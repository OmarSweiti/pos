//! Stock is a ledger (I-6): its events, and the cost basis each one captures
//! (microstep 1.10.2, `ref/domain-api.md` §12, `ref/schema.md` §0006).
//!
//! On-hand is the sum of the ledger's quantity deltas, and `stock_cache` is
//! that sum projected, rebuildable from the events. Every event also captures
//! the cost basis in force when it happened — on a sale as much as on a
//! delivery — so the margin a report derives in June is the margin January
//! recorded, whatever the weighted average cost (WAC) has become since. The
//! ledger is append-only, so there is no second chance to add it.
//!
//! [`StockEvent::capture`] is the one rule here: given the position a product
//! is in and the movement a caller asks for, the event that records it — its
//! cost basis and the position it leaves. `0006`'s projection trigger checks
//! the same rule in the storage engine, so a row this did not produce is
//! refused there too.
//!
//! * **An outgoing event carries the cost basis in force and leaves it
//!   unchanged.** A sale, waste or a transfer out takes no cost from its
//!   caller: what the stock cost is the ledger's answer, not the caller's.
//! * **An incoming event brings a cost**: observed, projected, or unknown.
//!   Into stock that is not positive, the WAC becomes that cost, because a
//!   negative on-hand carries no basis to average (master plan C.7). Into
//!   positive stock at the WAC already in force, the WAC stands.
//! * **Blending a different cost into positive stock is refused, not
//!   approximated.** That blend is the weighted average itself,
//!   `recompute_wac`, whose properties are 4.2.2's. Until it lands,
//!   [`StockError::WacBlendNotSupported`] says so by name.
//! * **An event that brings no cost never moves the WAC.** It marks the WAC
//!   estimated, and a report counts such events rather than showing an unknown
//!   margin as exact.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{ProductId, SaleId, SaleLineId, StockEventId, StoreId, UserId};
use crate::money::{Currency, Money, MoneyError, Qty};
use crate::time::{BusinessDate, Timestamp};

/// Why stock moved — `stock_ledger.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockEventKind {
    Sale,
    RefundRestock,
    RefundDamage,
    Receive,
    Adjust,
    CountCorrection,
    TransferOut,
    TransferIn,
    Waste,
    Rtv,
    KitExplode,
}

impl StockEventKind {
    /// Every kind, in `0006`'s `CHECK` order.
    pub const ALL: [StockEventKind; 11] = [
        StockEventKind::Sale,
        StockEventKind::RefundRestock,
        StockEventKind::RefundDamage,
        StockEventKind::Receive,
        StockEventKind::Adjust,
        StockEventKind::CountCorrection,
        StockEventKind::TransferOut,
        StockEventKind::TransferIn,
        StockEventKind::Waste,
        StockEventKind::Rtv,
        StockEventKind::KitExplode,
    ];

    /// The token `stock_ledger.kind` stores and an event's payload carries.
    pub const fn as_str(self) -> &'static str {
        match self {
            StockEventKind::Sale => "sale",
            StockEventKind::RefundRestock => "refund_restock",
            StockEventKind::RefundDamage => "refund_damage",
            StockEventKind::Receive => "receive",
            StockEventKind::Adjust => "adjust",
            StockEventKind::CountCorrection => "count_correction",
            StockEventKind::TransferOut => "transfer_out",
            StockEventKind::TransferIn => "transfer_in",
            StockEventKind::Waste => "waste",
            StockEventKind::Rtv => "rtv",
            StockEventKind::KitExplode => "kit_explode",
        }
    }
}

/// The document that moved stock, when one did.
///
/// One variant today. The refund, goods-receipt and stock-count sources arrive
/// with the microsteps that write those documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockSource {
    /// The sale line whose quantity left the shelf.
    SaleLine {
        sale_id: SaleId,
        line_id: SaleLineId,
    },
}

/// What an incoming event's stock cost, per unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomingCost {
    /// Read from a document: a supplier invoice, an opening-stock sheet.
    Observed(Money),
    /// Projected rather than observed, and reported as estimated.
    Projected(Money),
    /// No cost basis exists. Stored as `NULL`, never as zero: a real zero is
    /// `Observed(zero)`, and collapsing the two would make an unknown margin
    /// look exact.
    Unknown,
}

/// A product's stock in one store, as `stock_cache` holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockPosition {
    pub on_hand: Qty,
    /// The weighted average cost per unit.
    pub wac: Money,
    /// Whether any cost behind `wac` was projected or missing.
    pub is_wac_estimated: bool,
}

impl StockPosition {
    /// A product the store has never stocked: nothing on hand and no cost
    /// basis, which is estimated by definition. These are the defaults `0006`'s
    /// projection trigger reads for a product with no cache row.
    pub fn unstocked(currency: Currency) -> StockPosition {
        StockPosition {
            on_hand: Qty::ZERO,
            wac: Money::zero(currency),
            is_wac_estimated: true,
        }
    }

    /// The cost a price override's floor is held to (1.4.7): the weighted
    /// average cost, or `None` when no cost was ever recorded.
    ///
    /// A product with no recorded cost holds a WAC of zero marked estimated,
    /// as [`StockPosition::unstocked`] does, because an event that brings no
    /// cost never moves the WAC. Read as a cost, that zero would put the floor
    /// at nothing, so it is no cost at all. A projected zero reads the same
    /// way, which only ever refuses more. An estimated WAC above zero is a
    /// blend of the costs that were recorded, and the best cost known.
    pub fn cost_basis(&self) -> Option<Money> {
        (!(self.is_wac_estimated && self.wac.is_zero())).then_some(self.wac)
    }
}

/// A movement a caller asks the ledger to record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewStockEvent {
    pub id: StockEventId,
    pub product_id: ProductId,
    pub store_id: StoreId,
    pub kind: StockEventKind,
    /// Signed: negative leaves the shelf, positive arrives.
    pub qty_delta: Qty,
    /// What the arriving stock cost. Required on an incoming event and refused
    /// on any other, whose cost is the basis in force.
    pub incoming_cost: Option<IncomingCost>,
    /// The quantity is the weight a price-embedded label implies rather than
    /// a measurement (§4.3). Only a sale's label can say that.
    pub is_weight_derived: bool,
    pub source: Option<StockSource>,
    pub actor_id: Option<UserId>,
    pub occurred_at: Timestamp,
    pub business_date: BusinessDate,
}

/// A stock event as the ledger stores it: the movement, the cost basis it
/// captured, and the position it left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockEvent {
    pub id: StockEventId,
    pub product_id: ProductId,
    pub store_id: StoreId,
    pub kind: StockEventKind,
    pub qty_delta: Qty,
    /// The cost basis in force at this event, on every kind including `Sale`.
    /// `None` means no basis existed.
    pub unit_cost: Option<Money>,
    /// The cost was projected rather than observed, or no basis existed. It is
    /// true whenever `unit_cost` is `None`.
    pub is_cost_estimated: bool,
    pub is_weight_derived: bool,
    pub source: Option<StockSource>,
    pub actor_id: Option<UserId>,
    pub occurred_at: Timestamp,
    pub business_date: BusinessDate,
    /// The position this event leaves, which is what the cache holds after it.
    pub after: StockPosition,
}

/// Why a movement cannot be recorded.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StockError {
    /// A sale event names the sale line it moved, and only a sale event names
    /// one.
    #[error("a sale event must name its sale line, and only a sale event may")]
    SaleSourceMismatch,
    #[error("a sale event moves stock out")]
    SaleNotOutgoing,
    #[error("only a sale's price-embedded label can derive a weight")]
    WeightDerivedOutsideSale,
    #[error("an incoming event must say what its stock cost")]
    IncomingCostMissing,
    #[error("only an incoming event brings a cost; any other carries the basis in force")]
    CostOnOutgoingEvent,
    #[error("a unit cost cannot be negative")]
    NegativeCost,
    /// Positive stock at one cost, arriving stock at another: the weighted
    /// average, which is 4.2.2's.
    #[error(
        "blending a unit cost of {} into positive stock valued at {} ({}, in minor units) needs \
         the weighted average, which this build does not compute",
        .incoming.minor(),
        .wac.minor(),
        .wac.currency().code()
    )]
    WacBlendNotSupported { wac: Money, incoming: Money },
    /// The store sells nothing it does not have, and a basket would take
    /// these products below zero (master plan C.7's hard block). Refused
    /// before tender, never once money has been taken.
    #[error("the store sells no stock below zero, and {} product(s) would go below it", .0.len())]
    WouldGoNegative(Vec<Shortfall>),
    #[error(transparent)]
    Money(#[from] MoneyError),
}

impl StockEvent {
    /// The event that records `new` against `before`, the position the product
    /// is in: the cost basis it captures and the position it leaves.
    ///
    /// # Errors
    ///
    /// [`StockError`] when the movement is malformed — a sale without its line,
    /// a cost on an outgoing event or none on an incoming one, a cost in
    /// another currency or below zero — or when recording it would need a WAC
    /// blend, or the quantity overflows.
    pub fn capture(new: &NewStockEvent, before: &StockPosition) -> Result<StockEvent, StockError> {
        let is_sale = new.kind == StockEventKind::Sale;
        let names_a_sale_line = matches!(new.source, Some(StockSource::SaleLine { .. }));
        if is_sale != names_a_sale_line {
            return Err(StockError::SaleSourceMismatch);
        }
        if is_sale && new.qty_delta.milli() >= 0 {
            return Err(StockError::SaleNotOutgoing);
        }
        if new.is_weight_derived && !is_sale {
            return Err(StockError::WeightDerivedOutsideSale);
        }

        let on_hand = before.on_hand.checked_add(new.qty_delta)?;
        let incoming = new.qty_delta.milli() > 0;
        let (unit_cost, is_cost_estimated, wac, is_wac_estimated) =
            match (incoming, new.incoming_cost) {
                (false, Some(_)) => return Err(StockError::CostOnOutgoingEvent),
                (true, None) => return Err(StockError::IncomingCostMissing),
                // Out, or no movement at all: the basis in force, unchanged.
                (false, None) => (
                    Some(before.wac),
                    before.is_wac_estimated,
                    before.wac,
                    before.is_wac_estimated,
                ),
                (true, Some(IncomingCost::Unknown)) => (None, true, before.wac, true),
                (true, Some(IncomingCost::Observed(cost) | IncomingCost::Projected(cost))) => {
                    if cost.currency() != before.wac.currency() {
                        return Err(MoneyError::CurrencyMismatch(
                            before.wac.currency().code(),
                            cost.currency().code(),
                        )
                        .into());
                    }
                    if cost.minor() < 0 {
                        return Err(StockError::NegativeCost);
                    }
                    let projected = matches!(new.incoming_cost, Some(IncomingCost::Projected(_)));
                    if before.on_hand.milli() <= 0 {
                        (Some(cost), projected, cost, projected)
                    } else if cost == before.wac {
                        (
                            Some(cost),
                            projected,
                            cost,
                            projected || before.is_wac_estimated,
                        )
                    } else {
                        return Err(StockError::WacBlendNotSupported {
                            wac: before.wac,
                            incoming: cost,
                        });
                    }
                }
            };

        Ok(StockEvent {
            id: new.id,
            product_id: new.product_id,
            store_id: new.store_id,
            kind: new.kind,
            qty_delta: new.qty_delta,
            unit_cost,
            is_cost_estimated,
            is_weight_derived: new.is_weight_derived,
            source: new.source,
            actor_id: new.actor_id,
            occurred_at: new.occurred_at,
            business_date: new.business_date,
            after: StockPosition {
                on_hand,
                wac,
                is_wac_estimated,
            },
        })
    }
}

/// A product's on-hand in a store: the sum of its events' quantity deltas
/// (I-6). `events` are that product's events in that store; the sum is the
/// definition, so it neither checks them nor reads any event's projection.
///
/// `stock_cache` holds this figure and is rebuilt to it (microstep 1.10.3).
/// Each event also stores the position it left, and over a ledger whose events
/// were captured in order the last one's `after.on_hand` is this sum; the
/// sum stays the definition, because a projection is what a rebuild checks.
///
/// # Errors
///
/// [`StockError::Money`] when the sum leaves the range of a quantity.
pub fn on_hand(events: &[StockEvent]) -> Result<Qty, StockError> {
    events.iter().try_fold(
        Qty::ZERO,
        |sum, event| Ok(sum.checked_add(event.qty_delta)?),
    )
}

/// Whether a store lets a sale take stock below zero — `store.allow_negative_stock`
/// (master plan C.7, merchant decision 4.1).
///
/// The default allows it and flags it loudly. Refusing a sale because the
/// ledger is wrong punishes the customer at the register for a back-office
/// error, and until receiving arrives a ledger is wrong more often than not. A
/// tightly run store may choose the hard block instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NegativeStockPolicy {
    AllowAndFlag,
    HardBlock,
}

/// A product a basket would leave below zero: what is on hand, and how much
/// of it the basket's lines take between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortfall {
    pub product_id: ProductId,
    pub on_hand: Qty,
    pub taken: Qty,
}

impl NegativeStockPolicy {
    /// Screen what a basket takes from stock, before it is tendered.
    ///
    /// `taken` is per line, as the stock each line will move: lines of one
    /// product are summed, so the screen is the same however the basket was
    /// rung up. `on_hand` holds each product's position; one it does not hold
    /// has none, as a product never stocked has none. Services move no stock,
    /// so the caller leaves them out.
    ///
    /// Returns every product the basket takes from and would leave below zero,
    /// in product order — the flag. Selling the last unit leaves zero, which is
    /// not below it.
    ///
    /// # Errors
    ///
    /// [`StockError::WouldGoNegative`] under [`NegativeStockPolicy::HardBlock`]
    /// when the flag is not empty, carrying it; [`StockError::Money`] when a
    /// sum leaves the range of a quantity.
    pub fn screen(
        self,
        on_hand: &BTreeMap<ProductId, Qty>,
        taken: &[(ProductId, Qty)],
    ) -> Result<Vec<Shortfall>, StockError> {
        let mut basket: BTreeMap<ProductId, Qty> = BTreeMap::new();
        for &(product_id, qty) in taken {
            let sum = basket.entry(product_id).or_insert(Qty::ZERO);
            *sum = sum.checked_add(qty)?;
        }
        let mut shortfalls = Vec::new();
        for (product_id, taken) in basket {
            let held = on_hand.get(&product_id).copied().unwrap_or(Qty::ZERO);
            if taken.milli() > 0 && held.checked_sub(taken)?.milli() < 0 {
                shortfalls.push(Shortfall {
                    product_id,
                    on_hand: held,
                    taken,
                });
            }
        }
        match self {
            NegativeStockPolicy::HardBlock if !shortfalls.is_empty() => {
                Err(StockError::WouldGoNegative(shortfalls))
            }
            _ => Ok(shortfalls),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use pos_test_support::domain_proptest_config;
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;

    const JOD: Currency = Currency::JOD;

    fn jod(minor: i64) -> Money {
        Money::from_minor(minor, JOD)
    }

    fn position(on_hand_milli: i64, wac_minor: i64, is_wac_estimated: bool) -> StockPosition {
        StockPosition {
            on_hand: Qty::from_milli(on_hand_milli),
            wac: jod(wac_minor),
            is_wac_estimated,
        }
    }

    fn movement(kind: StockEventKind, qty_milli: i64, cost: Option<IncomingCost>) -> NewStockEvent {
        NewStockEvent {
            id: StockEventId::from_uuid(Uuid::from_u128(1)),
            product_id: ProductId::from_uuid(Uuid::from_u128(2)),
            store_id: StoreId::from_uuid(Uuid::from_u128(3)),
            kind,
            qty_delta: Qty::from_milli(qty_milli),
            incoming_cost: cost,
            is_weight_derived: false,
            source: None,
            actor_id: None,
            occurred_at: Timestamp::parse_iso8601("2026-10-25T09:00:00.000Z").unwrap(),
            business_date: BusinessDate::new(2026, 10, 25).unwrap(),
        }
    }

    fn sale(qty_milli: i64) -> NewStockEvent {
        NewStockEvent {
            source: Some(StockSource::SaleLine {
                sale_id: SaleId::from_uuid(Uuid::from_u128(4)),
                line_id: SaleLineId::from_uuid(Uuid::from_u128(5)),
            }),
            ..movement(StockEventKind::Sale, qty_milli, None)
        }
    }

    #[test]
    fn stock_event_kind_tokens_are_its_serde_names() {
        for kind in StockEventKind::ALL {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::Value::from(kind.as_str())
            );
        }
    }

    #[test]
    fn a_sale_captures_the_cost_basis_in_force() {
        let event = StockEvent::capture(&sale(-2_000), &position(10_000, 450, false)).unwrap();
        assert_eq!(event.unit_cost, Some(jod(450)));
        assert!(!event.is_cost_estimated);
        assert_eq!(event.after, position(8_000, 450, false));
    }

    #[test]
    fn a_sale_of_unstocked_goods_is_estimated_at_zero_and_goes_negative() {
        let event = StockEvent::capture(&sale(-1_000), &StockPosition::unstocked(JOD)).unwrap();
        assert_eq!(event.unit_cost, Some(jod(0)));
        assert!(event.is_cost_estimated);
        assert_eq!(event.after, position(-1_000, 0, true));
    }

    #[test]
    fn opening_stock_sets_the_wac_to_its_own_cost() {
        let observed = movement(
            StockEventKind::Adjust,
            12_000,
            Some(IncomingCost::Observed(jod(380))),
        );
        let event = StockEvent::capture(&observed, &StockPosition::unstocked(JOD)).unwrap();
        assert_eq!(
            (event.unit_cost, event.is_cost_estimated),
            (Some(jod(380)), false)
        );
        assert_eq!(event.after, position(12_000, 380, false));

        // Into negative stock as well: a negative on-hand has no basis to blend.
        let event = StockEvent::capture(&observed, &position(-3_000, 999, true)).unwrap();
        assert_eq!(event.after, position(9_000, 380, false));
    }

    #[test]
    fn missing_cost_is_null_estimated_and_leaves_the_wac() {
        let unknown = movement(StockEventKind::Adjust, 5_000, Some(IncomingCost::Unknown));
        let event = StockEvent::capture(&unknown, &position(2_000, 450, false)).unwrap();
        assert_eq!((event.unit_cost, event.is_cost_estimated), (None, true));
        assert_eq!(event.after, position(7_000, 450, true));
    }

    /// What a price override's floor is held to (1.4.7), read through the
    /// ledger rather than from positions typed by hand.
    #[test]
    fn a_product_never_given_a_cost_has_no_cost_basis() {
        let unstocked = StockPosition::unstocked(JOD);
        assert_eq!(unstocked.cost_basis(), None);
        // Arriving with no cost leaves the zero in place, still no cost.
        let unknown = movement(StockEventKind::Adjust, 5_000, Some(IncomingCost::Unknown));
        let uncosted = StockEvent::capture(&unknown, &unstocked).unwrap().after;
        assert_eq!(uncosted, position(5_000, 0, true));
        assert_eq!(uncosted.cost_basis(), None);
        // A recorded cost is the basis, and stays it when uncosted stock joins.
        let observed = movement(
            StockEventKind::Adjust,
            12_000,
            Some(IncomingCost::Observed(jod(380))),
        );
        let costed = StockEvent::capture(&observed, &unstocked).unwrap().after;
        assert_eq!(costed.cost_basis(), Some(jod(380)));
        let blended = StockEvent::capture(&unknown, &costed).unwrap().after;
        assert!(blended.is_wac_estimated);
        assert_eq!(blended.cost_basis(), Some(jod(380)));
        // A real zero, observed on a document, is a cost of nothing.
        let free = movement(
            StockEventKind::Adjust,
            1_000,
            Some(IncomingCost::Observed(jod(0))),
        );
        let given = StockEvent::capture(&free, &unstocked).unwrap().after;
        assert_eq!(given.cost_basis(), Some(jod(0)));
    }

    #[test]
    fn arriving_at_the_wac_in_force_keeps_it() {
        let projected = movement(
            StockEventKind::CountCorrection,
            1_000,
            Some(IncomingCost::Projected(jod(450))),
        );
        let event = StockEvent::capture(&projected, &position(2_000, 450, false)).unwrap();
        assert_eq!(
            (event.unit_cost, event.is_cost_estimated),
            (Some(jod(450)), true)
        );
        assert_eq!(event.after, position(3_000, 450, true));
    }

    #[test]
    fn a_different_cost_into_positive_stock_is_refused_not_blended() {
        let receipt = movement(
            StockEventKind::Receive,
            1_000,
            Some(IncomingCost::Observed(jod(500))),
        );
        assert_eq!(
            StockEvent::capture(&receipt, &position(2_000, 450, false)),
            Err(StockError::WacBlendNotSupported {
                wac: jod(450),
                incoming: jod(500)
            })
        );
        // Read by an operator, so it names the amounts, not the types.
        assert_eq!(
            StockEvent::capture(&receipt, &position(2_000, 450, false))
                .unwrap_err()
                .to_string(),
            "blending a unit cost of 500 into positive stock valued at 450 (JOD, in minor units) \
             needs the weighted average, which this build does not compute"
        );
    }

    #[test]
    fn a_malformed_movement_is_refused_by_name() {
        let before = position(5_000, 450, false);
        let refused = |event: NewStockEvent| StockEvent::capture(&event, &before).unwrap_err();

        assert_eq!(
            refused(movement(StockEventKind::Sale, -1_000, None)),
            StockError::SaleSourceMismatch
        );
        assert_eq!(
            refused(NewStockEvent {
                kind: StockEventKind::Waste,
                ..sale(-1_000)
            }),
            StockError::SaleSourceMismatch
        );
        assert_eq!(refused(sale(0)), StockError::SaleNotOutgoing);
        assert_eq!(
            refused(NewStockEvent {
                is_weight_derived: true,
                ..movement(StockEventKind::Waste, -1_000, None)
            }),
            StockError::WeightDerivedOutsideSale
        );
        assert_eq!(
            refused(movement(StockEventKind::Adjust, 1_000, None)),
            StockError::IncomingCostMissing
        );
        assert_eq!(
            refused(movement(
                StockEventKind::Waste,
                -1_000,
                Some(IncomingCost::Unknown)
            )),
            StockError::CostOnOutgoingEvent
        );
        assert_eq!(
            refused(movement(
                StockEventKind::Adjust,
                1_000,
                Some(IncomingCost::Observed(jod(-1)))
            )),
            StockError::NegativeCost
        );
        assert_eq!(
            refused(movement(
                StockEventKind::Adjust,
                1_000,
                Some(IncomingCost::Observed(Money::from_minor(
                    450,
                    Currency::USD
                )))
            )),
            StockError::Money(MoneyError::CurrencyMismatch("JOD", "USD"))
        );
        // One milli-unit below empty, minus the most a quantity can hold.
        assert_eq!(
            StockEvent::capture(
                &movement(StockEventKind::Waste, i64::MIN, None),
                &position(-1, 450, false)
            ),
            Err(StockError::Money(MoneyError::Overflow))
        );
    }

    #[test]
    fn on_hand_is_the_sum_of_the_deltas() {
        assert_eq!(on_hand(&[]), Ok(Qty::ZERO));
        let opening = StockEvent::capture(
            &movement(
                StockEventKind::Adjust,
                12_000,
                Some(IncomingCost::Observed(jod(380))),
            ),
            &StockPosition::unstocked(JOD),
        )
        .unwrap();
        let sold = StockEvent::capture(&sale(-15_000), &opening.after).unwrap();
        assert_eq!(on_hand(&[opening, sold]), Ok(Qty::from_milli(-3_000)));
    }

    #[test]
    fn on_hand_beyond_a_quantity_is_refused() {
        let full = StockEvent::capture(
            &movement(
                StockEventKind::Adjust,
                i64::MAX,
                Some(IncomingCost::Unknown),
            ),
            &StockPosition::unstocked(JOD),
        )
        .unwrap();
        assert_eq!(
            on_hand(&[full.clone(), full]),
            Err(StockError::Money(MoneyError::Overflow))
        );
    }

    fn product(n: u128) -> ProductId {
        ProductId::from_uuid(Uuid::from_u128(n))
    }

    #[test]
    fn a_basket_is_screened_product_by_product() {
        let on_hand = BTreeMap::from([
            (product(1), Qty::from_milli(1_000)),
            (product(2), Qty::from_milli(5_000)),
        ]);
        // Two lines of the first product take three units between them; the
        // last unit of the second leaves zero; a third was never stocked.
        let taken = [
            (product(1), Qty::from_milli(1_000)),
            (product(2), Qty::from_milli(5_000)),
            (product(3), Qty::from_milli(250)),
            (product(1), Qty::from_milli(2_000)),
        ];
        let flagged = vec![
            Shortfall {
                product_id: product(1),
                on_hand: Qty::from_milli(1_000),
                taken: Qty::from_milli(3_000),
            },
            Shortfall {
                product_id: product(3),
                on_hand: Qty::ZERO,
                taken: Qty::from_milli(250),
            },
        ];
        assert_eq!(
            NegativeStockPolicy::AllowAndFlag.screen(&on_hand, &taken),
            Ok(flagged.clone())
        );
        assert_eq!(
            NegativeStockPolicy::HardBlock.screen(&on_hand, &taken),
            Err(StockError::WouldGoNegative(flagged))
        );
        let last_unit = [(product(2), Qty::from_milli(5_000))];
        assert_eq!(
            NegativeStockPolicy::HardBlock.screen(&on_hand, &last_unit),
            Ok(vec![])
        );
    }

    #[test]
    fn stock_already_below_zero_is_flagged_only_when_the_basket_takes_from_it() {
        let on_hand = BTreeMap::from([(product(1), Qty::from_milli(-2_000))]);
        assert_eq!(
            NegativeStockPolicy::HardBlock.screen(&on_hand, &[(product(1), Qty::ZERO)]),
            Ok(vec![])
        );
        assert_eq!(
            NegativeStockPolicy::HardBlock.screen(&on_hand, &[(product(1), Qty::from_milli(1))]),
            Err(StockError::WouldGoNegative(vec![Shortfall {
                product_id: product(1),
                on_hand: Qty::from_milli(-2_000),
                taken: Qty::from_milli(1),
            }]))
        );
    }

    /// One movement of a sequence: a signed number of units, and the cost an
    /// arrival brings. Outgoing and zero movements bring none.
    fn any_movement() -> impl Strategy<Value = NewStockEvent> {
        (-50i64..=50, any_cost()).prop_map(|(units, cost)| {
            let qty = units * 1_000;
            movement(StockEventKind::Adjust, qty, (qty > 0).then_some(cost))
        })
    }

    fn any_position() -> impl Strategy<Value = StockPosition> {
        (
            -1_000_000_000i64..1_000_000_000,
            0i64..10_000_000,
            any::<bool>(),
        )
            .prop_map(|(on_hand, wac, estimated)| position(on_hand, wac, estimated))
    }

    fn any_cost() -> impl Strategy<Value = IncomingCost> {
        prop_oneof![
            (0i64..10_000_000).prop_map(|minor| IncomingCost::Observed(jod(minor))),
            (0i64..10_000_000).prop_map(|minor| IncomingCost::Projected(jod(minor))),
            Just(IncomingCost::Unknown),
        ]
    }

    proptest! {
        #![proptest_config(domain_proptest_config())]

        /// Whatever leaves the shelf carries the cost basis that was in force,
        /// and leaves that basis exactly as it was: a sale in January keeps
        /// January's cost, and selling never revalues what is left.
        #[test]
        fn prop_an_outgoing_event_carries_the_basis_in_force(
            before in any_position(),
            qty in -1_000_000_000i64..=0,
        ) {
            let event = StockEvent::capture(&movement(StockEventKind::Waste, qty, None), &before).unwrap();
            prop_assert_eq!(event.unit_cost, Some(before.wac));
            prop_assert_eq!(event.is_cost_estimated, before.is_wac_estimated);
            prop_assert_eq!(event.after.wac, before.wac);
            prop_assert_eq!(event.after.is_wac_estimated, before.is_wac_estimated);
            prop_assert_eq!(event.after.on_hand.milli(), before.on_hand.milli() + qty);
        }

        /// The WAC after any event is a cost somebody recorded — the one in
        /// force or the one that just arrived — never a figure the ledger made
        /// up; and an estimate, once in the WAC, stays flagged while that WAC
        /// stands.
        #[test]
        fn prop_the_wac_after_an_event_is_a_recorded_cost(
            before in any_position(),
            qty in 1i64..1_000_000_000,
            cost in any_cost(),
        ) {
            let arrival = movement(StockEventKind::Adjust, qty, Some(cost));
            match StockEvent::capture(&arrival, &before) {
                Ok(event) => {
                    let arrived = match cost {
                        IncomingCost::Observed(c) | IncomingCost::Projected(c) => Some(c),
                        IncomingCost::Unknown => None,
                    };
                    prop_assert!(event.after.wac == before.wac || Some(event.after.wac) == arrived);
                    prop_assert_eq!(event.unit_cost, arrived);
                    prop_assert_eq!(event.is_cost_estimated, !matches!(cost, IncomingCost::Observed(_)));
                    if event.after.wac == before.wac && before.on_hand.milli() > 0 {
                        prop_assert!(!before.is_wac_estimated || event.after.is_wac_estimated);
                    }
                    prop_assert_eq!(event.after.on_hand.milli(), before.on_hand.milli() + qty);
                }
                Err(StockError::WacBlendNotSupported { wac, incoming }) => {
                    prop_assert!(before.on_hand.milli() > 0);
                    prop_assert_eq!(wac, before.wac);
                    prop_assert_ne!(incoming, before.wac);
                }
                Err(other) => prop_assert!(false, "unexpected refusal {other:?}"),
            }
        }

        /// Over any run of events captured in order, the sum of what moved is
        /// the position the last event left. That is why a cache rebuilt by
        /// summing the ledger and a cache projected one event at a time can
        /// agree at all. A movement the ledger refuses is not an event and
        /// moves nothing.
        /// A store that allows negative stock never refuses a basket, and
        /// flags exactly the products whose lines, summed, take more than is
        /// on hand; a store that blocks it refuses exactly those baskets, and
        /// names the same products. However the basket was rung up — in any
        /// order, one line or several — the answer is the same.
        #[test]
        fn prop_hard_block_refuses_exactly_what_allow_and_flag_flags(
            held in prop::collection::btree_map(0u128..6, -5_000i64..5_000, 0..6),
            lines in prop::collection::vec((0u128..6, 0i64..4_000), 0..10),
        ) {
            let on_hand: BTreeMap<ProductId, Qty> =
                held.iter().map(|(&p, &q)| (product(p), Qty::from_milli(q))).collect();
            let taken: Vec<(ProductId, Qty)> =
                lines.iter().map(|&(p, q)| (product(p), Qty::from_milli(q))).collect();
            // The oracle: sum each product's lines by hand, in product order.
            let mut sums: BTreeMap<u128, i64> = BTreeMap::new();
            for &(p, q) in &lines {
                *sums.entry(p).or_insert(0) += q;
            }
            let expected: Vec<Shortfall> = sums
                .into_iter()
                .filter(|&(p, t)| t > 0 && held.get(&p).copied().unwrap_or(0) - t < 0)
                .map(|(p, t)| Shortfall {
                    product_id: product(p),
                    on_hand: Qty::from_milli(held.get(&p).copied().unwrap_or(0)),
                    taken: Qty::from_milli(t),
                })
                .collect();
            prop_assert_eq!(
                NegativeStockPolicy::AllowAndFlag.screen(&on_hand, &taken),
                Ok(expected.clone())
            );
            let mut reversed = taken.clone();
            reversed.reverse();
            let blocked = NegativeStockPolicy::HardBlock.screen(&on_hand, &reversed);
            if expected.is_empty() {
                prop_assert_eq!(blocked, Ok(vec![]));
            } else {
                prop_assert_eq!(blocked, Err(StockError::WouldGoNegative(expected)));
            }
        }

        #[test]
        fn prop_on_hand_is_the_position_the_last_event_leaves(
            movements in prop::collection::vec(any_movement(), 0..40),
        ) {
            let mut events: Vec<StockEvent> = Vec::new();
            let mut position = StockPosition::unstocked(JOD);
            for movement in &movements {
                match StockEvent::capture(movement, &position) {
                    Ok(event) => {
                        position = event.after;
                        events.push(event);
                    }
                    Err(StockError::WacBlendNotSupported { .. }) => {}
                    Err(other) => prop_assert!(false, "unexpected refusal {other:?}"),
                }
            }
            prop_assert_eq!(on_hand(&events), Ok(position.on_hand));
        }
    }
}
