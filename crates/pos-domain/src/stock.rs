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
    }
}
