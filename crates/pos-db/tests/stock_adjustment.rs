//! Microstep 1.10.5 — stock moved by hand with a manager's approval, and
//! opening stock posted as ledger events.
//!
//! A manager asks; the owner approves exactly what was asked; the effect posts
//! the event, its audit row and the approval's consumption together, under the
//! id the request was stored with. Opening stock is a provisioning path through
//! the same append path, never a cache row.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod sale_world;

use pos_db::DbError;
use pos_db::repo::approval::ApprovalRepository;
use pos_db::repo::outbox::CommitEnvelope;
use pos_db::repo::stock::StockRepository;
use pos_db::repo::stock_adjustment::StockAdjustmentRepository;
use pos_db::seed::opening_stock::{
    OpeningStockLoad, OpeningStockRow, load_opening_stock, parse_csv,
};
use pos_domain::{
    ApprovalBinding, ApprovalHandle, ApprovalId, Authorized, Currency, EscalationPolicy, GrantSet,
    IntentColumn, IntentValue, PermissionError, PreparedIntent, PreparedIntentHash, ProductId, Qty,
    RegisterId, Role, SeqIdSource, StockAdjustmentReason, StockAdjustmentRequest, StockError,
    StockEventId, StockPosition, StoreId, UserId, authorize, cap,
};
use rusqlite::params;
use sale_world::{AT, MANAGER, REGISTER, World, at, day, jod, no_sabotage, uuid, write_envelope};
use uuid::Uuid;

/// The second person with `stock.adjust`, who approves what the manager asks.
const OWNER: u8 = 0x56;
const IMMUTABLE: &str = "prepared stock intent is immutable after approval";

fn world() -> World {
    let w = World::new();
    w.conn
        .execute(
            "INSERT INTO app_user (id, org_id, code, display_name, pin_hash, pin_set_at)
             VALUES (?1, ?2, 'OWN-1', 'Synthetic Owner', 'placeholder-not-a-hash', ?3)",
            params![uuid(OWNER).as_bytes().as_slice(), &w.chain.org, AT],
        )
        .unwrap();
    w
}

fn manager() -> UserId {
    UserId::from_uuid(uuid(MANAGER))
}

fn owner() -> UserId {
    UserId::from_uuid(uuid(OWNER))
}

fn register() -> RegisterId {
    RegisterId::from_uuid(uuid(REGISTER))
}

fn event(byte: u8) -> StockEventId {
    StockEventId::from_uuid(uuid(byte))
}

fn repo(w: &World) -> StockAdjustmentRepository<'_> {
    StockAdjustmentRepository::new(&w.conn)
}

fn request(
    id: u8,
    product: ProductId,
    qty_milli: i64,
    reason: StockAdjustmentReason,
    note: Option<&str>,
) -> StockAdjustmentRequest {
    StockAdjustmentRequest {
        stock_event_id: event(id),
        product_id: product,
        qty_delta: Qty::from_milli(qty_milli),
        reason,
        note: note.map(str::to_owned),
    }
}

/// `stock_adjust_prepare`, by the manager, committed.
fn ask(w: &World, req: &StockAdjustmentRequest) {
    let tx = w.conn.unchecked_transaction().unwrap();
    repo(w).prepare(&tx, req, manager(), at()).unwrap();
    tx.commit().unwrap();
}

fn binding(w: &World, id: StockEventId) -> Result<ApprovalBinding, DbError> {
    let tx = w.conn.unchecked_transaction().unwrap();
    repo(w).binding(&tx, id)
}

/// The owner approves the request as it is stored now: a handle issued
/// against the recomputed binding, stored with its envelope.
fn approve(w: &World, id: StockEventId, approval: u8) -> ApprovalHandle {
    let tx = w.conn.unchecked_transaction().unwrap();
    let bound = repo(w).binding(&tx, id).unwrap();
    let approver = authorize::<cap::StockAdjust>(
        owner(),
        &GrantSet::of_role(Role::Owner),
        None,
        &bound,
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    let handle = ApprovalHandle::issue::<cap::StockAdjust>(
        ApprovalId::from_uuid(uuid(approval)),
        manager(),
        &approver,
        &bound,
        "synthetic shelf count".to_owned(),
        at(),
        600_000,
        [approval; 16],
    )
    .unwrap();
    write_envelope(
        &w.conn,
        &tx,
        approval,
        &[(
            "approval_handle".to_owned(),
            *handle.id().as_uuid().as_bytes(),
            "{}".to_owned(),
        )],
    );
    ApprovalRepository::new(&w.conn)
        .insert(&tx, &handle)
        .unwrap();
    tx.commit().unwrap();
    handle
}

/// The manager's proof for `stock_adjust`, which always needs a second person.
fn proof(
    handle: Option<&ApprovalHandle>,
    bound: &ApprovalBinding,
) -> Result<Authorized<cap::StockAdjust>, PermissionError> {
    authorize::<cap::StockAdjust>(
        manager(),
        &GrantSet::of_role(Role::Manager),
        handle,
        bound,
        &EscalationPolicy::empty().with_escalation::<cap::StockAdjust>(),
        at(),
    )
}

/// `stock_adjust`: the effect prepared, its envelope, the effect, committed.
fn adjust(
    w: &World,
    id: StockEventId,
    approved: &Authorized<cap::StockAdjust>,
    commit: u8,
) -> Result<StockPosition, DbError> {
    let tx = w.conn.unchecked_transaction().unwrap();
    let effect = repo(w).prepare_effect(
        &tx,
        id,
        register(),
        w.store(),
        approved,
        Uuid::from_bytes([commit, 0xAD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        day(),
    )?;
    let facts: Vec<(String, [u8; 16], String)> = effect
        .facts()
        .into_iter()
        .map(|(entity, entity_id, payload)| (entity.to_owned(), entity_id, payload))
        .collect();
    write_envelope(&w.conn, &tx, commit, &facts);
    let after = repo(w).commit(&tx, &effect)?;
    tx.commit().unwrap();
    Ok(after)
}

/// Ask, approve and adjust, the way the three commands will.
fn approved_adjustment(
    w: &World,
    req: &StockAdjustmentRequest,
    approval: u8,
    commit: u8,
) -> StockPosition {
    ask(w, req);
    let handle = approve(w, req.stock_event_id, approval);
    let bound = binding(w, req.stock_event_id).unwrap();
    adjust(
        w,
        req.stock_event_id,
        &proof(Some(&handle), &bound).unwrap(),
        commit,
    )
    .unwrap()
}

fn open(w: &World, rows: Vec<OpeningStockRow>) -> usize {
    let ids = SeqIdSource::new(1_788_000_000_000, 0x0B);
    let tx = w.conn.unchecked_transaction().unwrap();
    let posted = load_opening_stock(
        &tx,
        &OpeningStockLoad {
            register: register(),
            store: w.store(),
            actor: Some(manager()),
            occurred_at: at(),
            business_date: day(),
            envelope: CommitEnvelope {
                commit_id: &[0x0B; 16],
                protocol_version: 1,
                schema_version: pos_db::SCHEMA_VERSION,
                producer_version: "stock-adjustment-test",
                created_at: AT,
            },
            ids: &ids,
        },
        rows,
    )
    .unwrap();
    tx.commit().unwrap();
    posted
}

/// Twelve items at 0.380 and five kilograms of cheese whose cost nobody knows.
fn opening(w: &World) -> Vec<OpeningStockRow> {
    parse_csv(&format!(
        "product_id,qty_milli,unit_cost_minor\n{},12000,380\n{},5000,\n",
        w.item.as_uuid(),
        w.cheese.as_uuid()
    ))
    .unwrap()
}

fn count(w: &World, sql: &str) -> i64 {
    w.conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn stock_on_hand_returns_the_current_projection() {
    let w = world();
    let stock = StockRepository::new(&w.conn);
    assert_eq!(
        stock.position(w.item, w.store()).unwrap(),
        StockPosition::unstocked(Currency::JOD),
        "a product never stocked holds nothing, at no known cost"
    );
    open(&w, opening(&w));
    assert_eq!(
        stock.position(w.item, w.store()).unwrap(),
        StockPosition {
            on_hand: Qty::from_milli(12_000),
            wac: jod(380),
            is_wac_estimated: false,
        }
    );
    assert_eq!(
        stock.position(w.cheese, w.store()).unwrap(),
        StockPosition {
            on_hand: Qty::from_milli(5_000),
            wac: jod(0),
            is_wac_estimated: true,
        }
    );
    let elsewhere = StoreId::from_uuid(uuid(0xEE));
    assert!(matches!(
        stock.position(w.item, elsewhere),
        Err(DbError::StockStoreUnknown(s)) if s == elsewhere
    ));
}

#[test]
fn unknown_stock_adjustment_reason_is_refused() {
    let w = world();
    assert_eq!(
        StockAdjustmentReason::from_code("correction"),
        Err(StockError::UnknownAdjustmentReason("correction".to_owned()))
    );
    // The schema refuses a code the enum does not name, whoever writes it.
    let raw = w.conn.execute(
        "INSERT INTO stock_adjustment_request
           (stock_event_id, product_id, qty_delta_milli, reason_code, note,
            requested_by, requested_at, content_hash)
         VALUES (?1, ?2, -1000, 'refund', NULL, ?3, ?4, zeroblob(32))",
        params![
            uuid(0x90).as_bytes().as_slice(),
            w.item.as_uuid().as_bytes().as_slice(),
            manager().as_uuid().as_bytes().as_slice(),
            AT
        ],
    );
    assert!(
        raw.unwrap_err()
            .to_string()
            .starts_with("CHECK constraint failed"),
        "reason_code is one of five"
    );
    // A reason holds its direction: damage that arrives is refused before it
    // is stored.
    let tx = w.conn.unchecked_transaction().unwrap();
    let arriving = request(0x91, w.item, 1_000, StockAdjustmentReason::Damage, None);
    assert!(matches!(
        repo(&w).prepare(&tx, &arriving, manager(), at()),
        Err(DbError::StockRefused(
            StockError::AdjustmentAgainstItsReason {
                reason: StockAdjustmentReason::Damage
            }
        ))
    ));
    tx.commit().unwrap();
    assert_eq!(
        count(&w, "SELECT count(*) FROM stock_adjustment_request"),
        0
    );
}

#[test]
fn stock_adjust_is_audited_with_a_reason_code() {
    let w = world();
    open(&w, opening(&w));
    let dropped = request(
        0x30,
        w.item,
        -2_000,
        StockAdjustmentReason::Damage,
        Some("crate dropped"),
    );
    let after = approved_adjustment(&w, &dropped, 0x20, 0x31);
    assert_eq!(
        after,
        StockPosition {
            on_hand: Qty::from_milli(10_000),
            wac: jod(380),
            is_wac_estimated: false,
        }
    );

    let (kind, reason, note, actor): (String, String, String, Vec<u8>) = w
        .conn
        .query_row(
            "SELECT kind, reason_code, note, actor_id FROM stock_ledger WHERE id = ?1",
            [uuid(0x30).as_bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (kind.as_str(), reason.as_str(), note.as_str()),
        ("adjust", "damage", "crate dropped")
    );
    assert_eq!(actor, manager().as_uuid().as_bytes().to_vec());

    let (action, entity, actor, approver, handle, why, payload): (
        String,
        String,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        String,
        String,
    ) = w
        .conn
        .query_row(
            "SELECT action, entity, actor_id, approver_id, approval_handle_id, reason, payload
               FROM audit_log WHERE entity_id = ?1",
            [uuid(0x30).as_bytes().as_slice()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        (action.as_str(), entity.as_str(), why.as_str()),
        ("stock.adjust", "stock_ledger", "synthetic shelf count")
    );
    assert_eq!(actor, manager().as_uuid().as_bytes().to_vec());
    assert_eq!(approver, owner().as_uuid().as_bytes().to_vec());
    assert_eq!(handle, uuid(0x20).as_bytes().to_vec());
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    let field = |key: &str| payload.get(key).cloned().unwrap();
    assert_eq!(field("reason_code"), "damage");
    assert_eq!(field("qty_delta_milli"), -2_000);
    assert_eq!(field("amount_minor"), 0);
}

#[test]
fn stock_adjust_approval_for_another_event_is_refused() {
    let w = world();
    open(&w, opening(&w));
    let first = request(0x30, w.item, -1_000, StockAdjustmentReason::Damage, None);
    let second = request(0x40, w.item, -1_000, StockAdjustmentReason::Expiry, None);
    ask(&w, &first);
    ask(&w, &second);
    let handle = approve(&w, first.stock_event_id, 0x20);
    let bound = binding(&w, first.stock_event_id).unwrap();
    let approved_first = proof(Some(&handle), &bound).unwrap();
    let changes = w.conn.total_changes();

    // The handle names the first event; spent on the second, the repository
    // holds it to the second's binding and refuses.
    assert!(matches!(
        adjust(&w, second.stock_event_id, &approved_first, 0x41),
        Err(DbError::ApprovalRefused(PermissionError::ApprovalEntityMismatch(named, asked)))
            if named == uuid(0x30) && asked == uuid(0x40)
    ));
    // A proof made without a second person is refused too: `stock_adjust` always needs one.
    let alone = authorize::<cap::StockAdjust>(
        manager(),
        &GrantSet::of_role(Role::Manager),
        None,
        &bound,
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    assert!(matches!(
        adjust(&w, first.stock_event_id, &alone, 0x42),
        Err(DbError::ApprovalRefused(
            PermissionError::EscalationRequired("stock.adjust")
        ))
    ));
    // A handle that was issued and never stored proves nothing.
    let unstored = approve_unstored(&bound);
    assert!(matches!(
        adjust(&w, first.stock_event_id, &proof(Some(&unstored), &bound).unwrap(), 0x43),
        Err(DbError::ApprovalMissing(id)) if id == unstored.id()
    ));
    assert_eq!(
        w.conn.total_changes(),
        changes,
        "every refusal wrote nothing"
    );

    adjust(&w, first.stock_event_id, &approved_first, 0x44).unwrap();
    assert_eq!(
        StockRepository::new(&w.conn)
            .on_hand(w.item, w.store())
            .unwrap(),
        Qty::from_milli(11_000)
    );
    assert_eq!(
        count(&w, "SELECT count(*) FROM stock_adjustment_request"),
        1
    );
}

/// An approval issued by the owner and never stored.
fn approve_unstored(bound: &ApprovalBinding) -> ApprovalHandle {
    let approver = authorize::<cap::StockAdjust>(
        owner(),
        &GrantSet::of_role(Role::Owner),
        None,
        bound,
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    ApprovalHandle::issue::<cap::StockAdjust>(
        ApprovalId::from_uuid(uuid(0x2F)),
        manager(),
        &approver,
        bound,
        "never stored".to_owned(),
        at(),
        600_000,
        [0x2F; 16],
    )
    .unwrap()
}

/// An approval binds the digest the request was stored with, and nothing can
/// change the request after it: every prepared column, each varied after the
/// owner approved it and the manager's proof was made. The schema refuses the
/// `UPDATE`; with that trigger gone, the repository's recomputed digest refuses
/// the effect. Two independent layers, each shown refusing every column.
#[test]
fn altering_a_stock_request_after_approval_is_refused() {
    let w = world();
    open(&w, opening(&w));
    let mutations: [(&str, String); 8] = [
        (
            "stock_event_id",
            "x'B0B0B0B0B0B0B0B0B0B0B0B0B0B0B0B0'".to_owned(),
        ),
        ("product_id", format!("x'{}'", w.tobacco.as_uuid().simple())),
        ("qty_delta_milli", "-3000".to_owned()),
        ("reason_code", "'theft'".to_owned()),
        ("note", "'changed'".to_owned()),
        ("requested_by", format!("x'{}'", owner().as_uuid().simple())),
        ("requested_at", "'2026-08-25T11:00:00.000Z'".to_owned()),
        ("content_hash", "zeroblob(32)".to_owned()),
    ];
    // The schema's first layer: an approval must carry the digest the request
    // was stored with, so one for any other content is refused as it is issued.
    let other = request(0x5F, w.item, -1_000, StockAdjustmentReason::Damage, None);
    ask(&w, &other);
    let elsewhere = ApprovalBinding {
        entity_id: uuid(0x5F),
        amount_minor: 0,
        content_hash: Some(PreparedIntentHash::of(
            PreparedIntent::StockAdjustment,
            &[IntentColumn::NotNull(IntentValue::Text("another request"))],
        )),
    };
    let approver = authorize::<cap::StockAdjust>(
        owner(),
        &GrantSet::of_role(Role::Owner),
        None,
        &elsewhere,
        &EscalationPolicy::empty(),
        at(),
    )
    .unwrap();
    let mismatched = ApprovalHandle::issue::<cap::StockAdjust>(
        ApprovalId::from_uuid(uuid(0x5E)),
        manager(),
        &approver,
        &elsewhere,
        "synthetic shelf count".to_owned(),
        at(),
        600_000,
        [0x5E; 16],
    )
    .unwrap();
    let tx = w.conn.unchecked_transaction().unwrap();
    write_envelope(
        &w.conn,
        &tx,
        0x5E,
        &[("approval_handle".to_owned(), [0x5E; 16], "{}".to_owned())],
    );
    let issued = ApprovalRepository::new(&w.conn).insert(&tx, &mismatched);
    assert!(
        matches!(&issued, Err(DbError::Sqlite(e))
            if e.to_string() == "stock-adjust approval must bind the prepared intent content hash"),
        "{issued:?}"
    );
    drop(tx);

    let mut proofs = Vec::new();
    for (n, _) in (0_u8..).zip(&mutations) {
        let id = 0x60 + n;
        let req = request(
            id,
            w.item,
            -1_000,
            StockAdjustmentReason::Damage,
            Some("counted"),
        );
        ask(&w, &req);
        let handle = approve(&w, req.stock_event_id, id + 0x20);
        let bound = binding(&w, req.stock_event_id).unwrap();
        proofs.push((id, proof(Some(&handle), &bound).unwrap()));
    }
    let update = |column: &str, value: &str, id: u8| {
        w.conn.execute(
            &format!(
                "UPDATE stock_adjustment_request SET {column} = {value} WHERE stock_event_id = ?1"
            ),
            [uuid(id).as_bytes().as_slice()],
        )
    };

    // The schema: every column frozen once an approval names the request.
    for ((column, value), (id, _)) in mutations.iter().zip(&proofs) {
        let refused = update(column, value, *id);
        assert!(
            refused.as_ref().is_err_and(|e| e.to_string() == IMMUTABLE),
            "{column}: {refused:?}"
        );
    }

    // The digest: with the trigger gone, as a damaged or edited file might
    // have it, each change reaches the row, and the effect the manager's proof
    // was made for is still refused.
    w.conn
        .execute_batch("DROP TRIGGER stock_adjustment_request_no_update_after_approval")
        .unwrap();
    for ((column, value), (id, approved)) in mutations.iter().zip(&proofs) {
        update(column, value, *id).unwrap();
        let now_at = if *column == "stock_event_id" {
            StockEventId::from_uuid(Uuid::from_bytes([0xB0; 16]))
        } else {
            event(*id)
        };
        assert!(
            matches!(binding(&w, now_at), Err(DbError::StockAdjustmentAltered(at)) if at == now_at),
            "{column}"
        );
        let refused = adjust(&w, now_at, approved, id + 0x40);
        assert!(
            matches!(&refused, Err(DbError::StockAdjustmentAltered(at)) if *at == now_at),
            "{column}: {refused:?}"
        );
    }
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM stock_ledger WHERE reason_code <> 'opening_stock'"
        ),
        0
    );
    assert_eq!(count(&w, "SELECT count(*) FROM approval_consumption"), 0);
}

#[test]
fn stock_effect_consumption_and_audit_share_the_persisted_event_id() {
    let w = world();
    open(&w, opening(&w));
    let found = request(
        0x50,
        w.item,
        3_000,
        StockAdjustmentReason::CountCorrection,
        None,
    );
    ask(&w, &found);
    let handle = approve(&w, found.stock_event_id, 0x21);
    let bound = binding(&w, found.stock_event_id).unwrap();
    let approved = proof(Some(&handle), &bound).unwrap();

    // A first attempt rolled back, as a power cut would: nothing of it stays.
    {
        let tx = w.conn.unchecked_transaction().unwrap();
        let effect = repo(&w)
            .prepare_effect(
                &tx,
                found.stock_event_id,
                register(),
                w.store(),
                &approved,
                Uuid::from_bytes([0x51, 0xAD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
                day(),
            )
            .unwrap();
        let facts: Vec<(String, [u8; 16], String)> = effect
            .facts()
            .into_iter()
            .map(|(e, id, p)| (e.to_owned(), id, p))
            .collect();
        write_envelope(&w.conn, &tx, 0x51, &facts);
        repo(&w).commit(&tx, &effect).unwrap();
    }
    assert_eq!(
        count(&w, "SELECT count(*) FROM stock_adjustment_request"),
        1
    );
    assert!(
        !ApprovalRepository::new(&w.conn)
            .is_consumed(handle.id())
            .unwrap()
    );
    assert_eq!(
        count(
            &w,
            "SELECT count(*) FROM stock_ledger WHERE reason_code = 'count_correction'"
        ),
        0
    );

    let after = adjust(&w, found.stock_event_id, &approved, 0x52).unwrap();
    // Found stock comes in at the cost basis in force, projected.
    assert_eq!(
        after,
        StockPosition {
            on_hand: Qty::from_milli(15_000),
            wac: jod(380),
            is_wac_estimated: true,
        }
    );
    let id = uuid(0x50).as_bytes().to_vec();
    let shared: (Vec<u8>, Vec<u8>, Vec<u8>) = w
        .conn
        .query_row(
            "SELECT e.id, c.effect_id, a.entity_id
               FROM stock_ledger e
               JOIN approval_consumption c ON c.effect_id = e.id
               JOIN audit_log a ON a.id = c.audit_log_id
              WHERE e.id = ?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(shared, (id.clone(), id.clone(), id));
    assert_eq!(
        count(&w, "SELECT count(*) FROM stock_adjustment_request"),
        0
    );
    assert_eq!(
        count(&w, "SELECT count(*) FROM approval_handle"),
        1,
        "the approval itself is kept as evidence"
    );
    assert!(
        ApprovalRepository::new(&w.conn)
            .is_consumed(handle.id())
            .unwrap()
    );
    assert!(matches!(
        adjust(&w, found.stock_event_id, &approved, 0x53),
        Err(DbError::StockAdjustmentNotFound(_))
    ));
}

/// 1.10.5's `Done when`: the fixture loads N milli-units, sells 1000, reports
/// N - 1000 on hand, and keeps the sale event's captured cost through a later
/// adjustment. Every cache row the load leaves points at the event that
/// projected it.
#[test]
fn opening_stock_loader_posts_ledger_events_not_cache_rows() {
    let w = world();
    // A cache row that names no ledger event is refused for the whole test.
    w.conn
        .execute_batch(
            "CREATE TEMP TRIGGER cache_row_without_its_event BEFORE INSERT ON stock_cache
             WHEN NOT EXISTS (SELECT 1 FROM stock_ledger e WHERE e.id IS NEW.last_event_id)
             BEGIN SELECT RAISE(ABORT, 'a cache row without its event'); END;",
        )
        .unwrap();
    assert_eq!(open(&w, opening(&w)), 2);

    let mut rows = w
        .conn
        .prepare(
            "SELECT e.kind, e.reason_code, e.qty_delta_milli, e.unit_cost_minor,
                    e.is_cost_estimated, c.on_hand_milli
               FROM stock_ledger e
               JOIN stock_cache c ON c.last_event_id = e.id
              ORDER BY e.event_seq",
        )
        .unwrap();
    let posted: Vec<(String, String, i64, Option<i64>, bool, i64)> = rows
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        posted,
        [
            (
                "adjust".to_owned(),
                "opening_stock".to_owned(),
                12_000,
                Some(380),
                false,
                12_000
            ),
            // No cost known: NULL and estimated, never a zero.
            (
                "adjust".to_owned(),
                "opening_stock".to_owned(),
                5_000,
                None,
                true,
                5_000
            ),
        ]
    );
    let tx = w.conn.unchecked_transaction().unwrap();
    assert_eq!(
        StockRepository::new(&w.conn)
            .verify_cache(&tx, w.store())
            .unwrap(),
        []
    );
    drop(tx);

    // Sell one item; the sale captures the cost in force.
    w.finalize(&w.plain(0x20, "REG01-000001"), 0x02, no_sabotage, false)
        .unwrap();
    let stock = StockRepository::new(&w.conn);
    assert_eq!(
        stock.on_hand(w.item, w.store()).unwrap(),
        Qty::from_milli(11_000)
    );
    let sale_cost = |w: &World| -> Option<i64> {
        w.conn
            .query_row(
                "SELECT unit_cost_minor FROM stock_ledger WHERE kind = 'sale'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(sale_cost(&w), Some(380));

    // A later adjustment moves stock and leaves the sale's cost as captured.
    let lost = request(0x30, w.item, -2_000, StockAdjustmentReason::Theft, None);
    approved_adjustment(&w, &lost, 0x22, 0x31);
    assert_eq!(
        stock.on_hand(w.item, w.store()).unwrap(),
        Qty::from_milli(9_000)
    );
    assert_eq!(sale_cost(&w), Some(380));
}

/// Without opening stock the first sale takes every product below zero, so
/// the flag fires on everything and means nothing. With it, the flag fires
/// only where a basket really takes more than the shelf holds.
#[test]
fn opening_stock_makes_the_negative_stock_flag_meaningful() {
    let w = world();
    let stock = StockRepository::new(&w.conn);
    let one_item = [(w.item, Qty::from_milli(1_000))];
    assert_eq!(
        stock.screen_basket(w.store(), &one_item).unwrap().len(),
        1,
        "before opening stock, a single item is flagged"
    );

    open(&w, opening(&w));
    assert_eq!(stock.screen_basket(w.store(), &one_item).unwrap(), []);
    let flagged = stock
        .screen_basket(w.store(), &[(w.item, Qty::from_milli(13_000))])
        .unwrap();
    assert_eq!(flagged.len(), 1, "thirteen of twelve is a real shortfall");
    assert_eq!(stock.negative_stock(w.store()).unwrap(), []);
}

#[test]
fn opening_stock_file_is_read_exactly() {
    let product = Uuid::from_u128(0x61);
    let rows = parse_csv(&format!(
        "product_id,qty_milli,unit_cost_minor\r\n{product},2500,450\r\n\r\n{product},500,\r\n"
    ))
    .unwrap();
    assert_eq!(
        rows,
        [
            OpeningStockRow {
                product_id: ProductId::from_uuid(product),
                qty_milli: 2_500,
                unit_cost_minor: Some(450),
            },
            OpeningStockRow {
                product_id: ProductId::from_uuid(product),
                qty_milli: 500,
                unit_cost_minor: None,
            },
        ]
    );
    for (text, line) in [
        ("sku,qty,cost\n".to_owned(), 1),
        (
            format!("product_id,qty_milli,unit_cost_minor\n{product},1000\n"),
            2,
        ),
        (
            "product_id,qty_milli,unit_cost_minor\nnot-a-uuid,1000,\n".to_owned(),
            2,
        ),
        (
            format!("product_id,qty_milli,unit_cost_minor\n{product},1000,\n{product},1.5,\n"),
            3,
        ),
        (
            format!("product_id,qty_milli,unit_cost_minor\n{product},1000,free\n"),
            2,
        ),
    ] {
        assert!(
            matches!(parse_csv(&text), Err(DbError::OpeningStockFileInvalid { line: l, .. }) if l == line),
            "{text:?}"
        );
    }
}
