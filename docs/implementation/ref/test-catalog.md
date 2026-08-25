# Test catalog — the coverage matrix

Master plan Parts E and J.4 enumerate 72 things that can happen. This file maps **every one** to a named test, an accepted risk with a written rationale, or an explicit out-of-scope with one.

**The rule (master plan J.0, applied to itself):** an edge case with no row here is a gap, not an absence. A row with no test and no rationale is an unfinished job.

Legend — **Ph**: phase the test lands in · **Kind**: `unit` · `prop` · `golden` · `integration` · `chaos` · `drill` (a manual, documented, timed procedure).

---

## Power, crash, and state

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 1 | Power cut mid-`Finalizing` | `interrupted_finalize_resumes_without_double_stock_event`, `…_without_double_outbox_row`, `finalize_is_atomic_under_injected_failure` | 1 | integration |
| 1b | Power cut **after** the receipt printed | **post-receipt power-cut drill** — the customer has paid and gone, so on restart the sale must be *there*. Case 1 cannot detect this on its own: a kill before commit correctly yields zero sales, and a lost acknowledged commit yields zero sales too. Enforced by `synchronous = FULL`, which `pos_db::open` refuses to run without | 1 | drill |
| 2 | Power cut during card `Tendering` | `unknown_triggers_status_query_before_any_retry`, `status_query_approved_attaches_tender` | 2 | unit |
| 3 | App killed with parked carts | `prop_park_resume_roundtrip_is_identity`, `parked_carts_survive_restart` | 1 | prop |
| 4 | SQLite `BadKey` — keychain wiped | `bad_key_yields_recovery_state_not_panic` + **keychain-loss restore drill** | 1 / 5 | unit + drill |
| 4b | A fact table is left writable — the audit trail, stock ledger or cash trail edited after the fact | `every_shipped_fact_table_refuses_update_and_delete`, `the_fact_table_list_has_no_duplicates_and_names_nothing_twice`, `a_fact_table_that_does_not_exist_yet_is_not_silently_counted` | 1 | integration |
| 5 | Disk full | `low_disk_blocks_new_sales_and_alarms` | 1 | integration |
| 6 | Clock skew / cashier changes system time | `prop_monotonic_clock_never_decreases`, `clock_jump_back_reports_anomaly` | 1 | prop |
| 7 | DST / timezone; Z day boundary belongs to the shift | `sale_at_0100_belongs_to_previous_business_date`, `z_belongs_to_the_shifts_business_date_not_the_wall_clock`, `business_date_survives_timezone_change` | 1 / 2 | unit |

## Offline and sync

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 8 | Offline for days | `deep_outbox_alarms_without_blocking_sales`, `offline_week_converges` | 3 | chaos |
| 9 | Product edited centrally while offline | `open_cart_keeps_captured_price_after_catalog_apply`, `finalized_sales_are_never_touched` | 3 | integration |
| 10 | Duplicate push after a network retry | `duplicate_batch_is_a_no_op`, `prop_apply_is_idempotent_under_any_replay_order` | 3 | prop |
| 11 | Partial batch failure / poison pill | `partial_failure_acks_per_item`, `poison_item_goes_to_dead_letter_without_blocking` | 3 | integration |
| 12 | Two registers sell the last unit offline | `two_offline_registers_selling_the_last_unit_both_succeed`, `both_sales_of_the_last_unit_stand_and_stock_goes_negative_flagged` | 1 / 3 | chaos |
| 13 | Register clone / restore from image | `device_id_collision_refuses_sync_with_a_named_error` | 3 | integration |

## Money and rounding

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 14 | Split cash+card with cash rounding | `prop_cash_rounding_only_on_final_cash_tender`, `card_charged_exact_unrounded_total` | 1 | prop |
| 15 | Partial card approval | `partial_approval_leaves_remaining_due`, `partial_then_abandon_reverses`, `reversal_failure_escalates_and_audits` | 2 | unit |
| 16 | Refund exceeding remaining refundable | `prop_cumulative_refunds_never_exceed_sold_qty` | 2 | prop |
| 17 | Change due but drawer lacks denominations | `paid_in_from_safe_adjusts_expected_cash` — correctness is unaffected; the count helper is the UX answer | 2 | unit |
| 18 | 0.000 JOD total (100% discount / full redemption) | `prop_zero_total_cart_is_valid`, `zero_due_tender_completes_and_issues_a_fiscal_doc` | 1 / 2 | prop |
| 19 | Negative-price line attempts | `prop_discount_never_makes_a_line_negative`, conformance rule `F-010` | 1 / 2 | prop |

## Card terminal

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 20 | Timeout after the customer tapped | `prop_no_input_sequence_yields_two_tenders_for_one_auth` | 2 | prop |
| 21 | Terminal offline / unpaired at Pay | `card_disabled_when_terminal_unreachable_cash_still_works` | 2 | unit |
| 22 | Refund to an expired/cancelled card | `refund_api_error_offers_store_credit_with_manager_approval` | 2 | unit |
| 23 | Settlement mismatch vs. PSP report | `settlement_report_lists_unmatched_separately_by_direction` + **card reconciliation drill** | 2 | integration + drill |

## Fiscal (JoFotara)

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 24 | API down at sale time | mock fault `connection_refused`; `backoff_has_jitter` | 2 | integration |
| 25 | Validation rejection | mock fault `400`; `rejection_dead_letters_verbatim_and_never_mutates_the_sale` | 2 | integration |
| 26 | Refund of a not-yet-cleared sale | `prop_credit_note_never_precedes_its_invoice` | 2 | prop |
| 27 | Duplicate submission after ambiguous timeout | mock fault `409 already exists`; `existing_qr_is_fetched_and_persisted` | 2 | integration |
| 28 | Wrong environment credentials | `production_build_refuses_mock_credentials`, `tin_mismatch_in_response_alarms` | 2 | unit |
| 29 | Merchant not in a mandatory wave | `disabled_profile_produces_no_queue_row`, `receipt_prints_without_qr_when_disabled` | 2 | unit |

## Returns and fraud

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 30 | Return of an exchanged item | `refund_of_an_exchanged_item_follows_the_chain` | 2 | integration |
| 31 | Serial refund abuse across two stores | `second_store_refund_of_the_same_receipt_is_refused_when_connected` | 4 | integration |
| | | ⚠️ **Accepted risk:** inside an offline window both can succeed. Mitigated by the connected check and the refunds-by-user report. **Disclosed to the merchant.** | | |
| 32 | Receiptless return of a never-sold item | `receiptless_denied_when_disabled`, `receiptless_respects_threshold_and_requires_manager` | 2 | unit |
| 33 | Price-override abuse | `override_report_groups_by_user_with_reasons`, `override_below_floor_is_denied` | 1 / 4 | unit |
| 34 | Refund after a price change | `refund_uses_original_price_after_a_price_change`, `prop_refund_uses_original_rate` | 2 | prop |
| 35 | No-sale drawer-open spikes | `no_sale_open_is_logged_and_counted`, `z_report_counts_no_sale_opens` | 2 | unit |

## Catalog and pricing

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 36 | Barcode collision | `by_barcode_returns_newest_active_on_collision`, `barcode_conflict_report_lists_both` | 1 / 4 | unit |
| 37 | Price changed while an item is in an open cart | `open_cart_keeps_captured_price_after_catalog_apply`, `reprice_cart_action_applies_new_prices` | 3 | integration |
| 38 | Product deactivated with stock remaining | `inactive_product_cannot_be_added_but_can_be_refunded` | 1 | unit |
| 39 | Unknown barcode scan | `unknown_barcode_offers_quick_add_or_department_sale`, `queue_never_stalls_on_unknown_code` | 1 | unit |
| 40 | Price-embedded barcode with a checksum error | `prop_corrupt_digit_never_parses_clean` | 1 | prop |
| 41 | Unicode names (Arabic + emoji) | `golden_receipt_ar_80mm`, `unicode_names_roundtrip_through_db_and_fts` | 1 | golden |
| 41b | Arabic typed in a different but equivalent spelling — tashkeel, أ/إ/آ for ا, ى for ي, ة for ه, tatweel | `fts_matches_arabic_with_and_without_diacritics`, `fts_matches_alef_and_yaa_spelling_variants`, `fts_matches_taa_marbuta_spelled_as_haa`, `fts_ignores_tatweel`, `fts_prefix_search_works_at_two_characters` | 1 | integration |

## Inventory

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 42 | Count during trading | `count_tolerates_sales_mid_count` | 4 | integration |
| 43 | Receiving with a fat-fingered cost | `ten_times_cost_requires_confirmation`, `corrective_adjust_recomputes_wac` | 4 | unit |
| 44 | Transfer arrives short or damaged | `short_receipt_creates_destination_adjustment_and_notifies` | 4 | integration |
| 45 | Expiry-dated goods | 🧩 **Deferred.** Lot tracking is a later module; v1 covers it with expiry-waste adjustments by reason. `waste_adjustment_by_reason_code` exists | 4 | unit |

## Documents and hardware

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 46 | Paper out mid-receipt | `print_failure_after_finalize_leaves_sale_complete`, `reprint_is_byte_identical_including_qr` | 1 / 2 | integration |
| 47 | Reprint days later from another register | `clearance_result_syncs_down_and_any_register_reprints` | 3 | integration |
| 48 | Email receipt bounce | `email_bounce_logged_without_retry_storm` — receipt remains printable | 3 | unit |
| 49 | 58 mm printer at a kiosk | `golden_receipt_ar_58mm`, `narrow_profile_reflows_rather_than_truncates` | 1 | golden |
| 50 | Drawer jammed/open at shift close | `jammed_drawer_does_not_block_shift_close` | 2 | unit |

## People and access

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 51 | Cashier forgets PIN | `manager_reset_retires_old_hash_and_audits` | 1 | unit |
| 52 | Manager approving their own sale | `manager_self_approval_denied_when_policy_bans_it` | 1 | unit |
| 53 | Shift left open overnight | `stale_shift_detected_and_force_closed_with_flag` | 2 | integration |
| 54 | Training mode left on | `training_excluded_from_reports_and_fiscal`, `training_auto_off_at_shift_close` | 1 / 2 | unit |
| 55 | Terminated employee's PIN | `deactivation_applies_at_next_contact`, `offline_auth_window_expires_and_says_why` | 3 | integration |
| | | ⚠️ **Accepted risk:** the PIN works until next contact or window expiry. **A real limit of offline-first; disclosed.** | | |

## Platform and lifecycle

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 56 | Auto-update mid-shift | `update_deferred_while_shift_open`, `failed_update_rolls_back` | 5 | integration |
| 57 | Licence expiry offline | `expired_licence_degrades_read_only_never_mid_day`, `grace_period_survives_a_long_outage` | 3 | unit |
| 58 | Migration failure on update | `half_migrated_db_refuses_to_open_with_a_named_error`, `all_migrations_run_against_soak_dataset_within_budget` | 1 / 5 | integration |
| 59 | Telemetry offline | `offline_telemetry_is_buffered_and_capped`, `no_pii_in_a_captured_panic` | 3 | unit |
| 60 | Multi-monitor / resolution chaos | `sale_screen_min_size_guard`, kiosk fullscreen mode | 1 | unit |

## J.4 additions — stored value, wallets, age, price control

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 61 | Gift card sold offline, redeemed elsewhere before sync | `stored_value_is_online_authorize_only_by_default` | 4 | unit |
| | | ⚠️ **Accepted risk when the offline cap is deliberately enabled.** Quantified in settings; off by default | | |
| 62 | Store credit issued offline at two stores | `prop_two_offline_registers_earning_converge` — ledgers are conflict-free; *redemption* checks the server balance when online | 3 | prop |
| 63 | Photocopied coupon code | `single_use_code_marked_used_on_redemption_sync` | 4 | integration |
| | | ⚠️ **Accepted risk** inside the offline window; surfaced in the promo report | | |
| 64 | E-recharge accepted but the app crashed before the receipt | 🧩 **Deferred with a hook.** When e-recharge ships, it reuses the card terminal's `Unknown` discipline: idempotency key per request, status-query before retry, never resell the same key | — | — |
| 65 | CliQ / wallet paid but callback lost | `pending_tender_polls_by_reference_before_declaring_unpaid`, `pending_tender_is_never_silently_dropped` | 2 | integration |
| 66 | E-recharge supplier down | 🧩 **Deferred with a hook.** `is_service` products become unsellable with an honest message — it is a service, not stock | — | — |
| 67 | Layaway lapses unpaid | 🚫 **Out of scope for v1.** Jordanian minimarkets rarely use layaway. Hook: `doc_type` variant + a payments ledger. Merchant decision #18 | — | — |
| 68 | Serialized return where the serial matches nothing sold | 🧩 **Deferred with a hook.** `sale_line_serial` table; the anti-swap control is a manager-override path. Phase 4+ for electronics merchants | — | — |
| 69 | Age check declined | `age_restricted_line_requires_confirmation`, `age_decline_removes_line_and_audits` | 1 / 2 | unit |
| 70 | Shelf tag says 0.99, system says 1.09 | `displayed_price_override_queues_a_label_reprint` | 1 / 4 | unit |
| 71 | Controlled staple above the ministry ceiling | `catalog_save_above_ceiling_is_rejected`, `sale_above_ceiling_is_hard_blocked` | 1 / 4 | unit |
| 72 | House account hits its credit limit while offline | 🚫 **Out of scope for v1.** Hook: customer credit limit + AR ledger + a per-customer offline exposure cap, same philosophy as 61. Merchant decision #18 | — | — |

---

## The invariant properties

The tests that matter most. Each states an invariant in the words a human would use, then lets `proptest` attack it.

| Property | Guards | Ph |
|---|---|---|
| `prop_split_preserves_total` *(exists)* | splitting a tender never changes the total | 0 |
| `prop_split_proportional_preserves_total` | proration conserves to the fil, for any weights | 1 |
| `prop_currency_mismatch_never_silently_coerces` | a JOD amount can never become a USD one | 1 |
| `prop_sql_and_rust_folding_agree` | the fold in the 0003 generated column and the fold the query path applies produce the same string, for any input — two implementations of one rule, so they will drift unless something holds them together | 1 |
| `prop_inclusive_net_plus_tax_equals_gross` | inclusive extraction is exact, at every rate | 1 |
| `prop_line_tax_sum_equals_receipt_tax` | the summary is the sum, never a re-derivation | 1 |
| `prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting` | the distinction survives to the filing report | 1 |
| `prop_total_equals_lines_minus_discounts_plus_tax` | the master identity | 1 |
| `prop_no_operation_mutates_a_complete_sale` | immutability, by construction | 1 |
| `prop_basket_discount_prorates_to_the_fil` | no fil is created or destroyed | 1 |
| `prop_discount_never_makes_a_line_negative` | ISTD rejects negative lines; so do we | 1 |
| `prop_cash_rounding_only_on_final_cash_tender` | card pays exact; cash rounds once | 1 |
| `prop_rounding_adjustment_keeps_total_exact` | the books still reconcile | 1 |
| `prop_cache_rebuild_matches_ledger` | the on-hand cache can never silently diverge | 1 |
| `prop_chain_detects_any_single_entry_mutation` | tamper-evidence actually evidences | 1 |
| `prop_monotonic_clock_never_decreases` | a changed system clock cannot reorder history | 1 |
| `prop_cumulative_refunds_never_exceed_sold_qty` | the anti-abuse core, in any order | 2 |
| `prop_no_input_sequence_yields_two_tenders_for_one_auth` | **no double charge, ever** | 2 |
| `prop_expected_cash_matches_movement_replay` | the drawer reconciles from first principles | 2 |
| `prop_z_number_is_gap_free` | an auditor's first question | 2 |
| `prop_discount_percentage_roundtrip_is_exact` | correction C-2 — JoFotara's per-line percentages | 2 |
| `prop_credit_note_never_precedes_its_invoice` | fiscal dependency ordering | 2 |
| `prop_apply_is_idempotent_under_any_replay_order` | sync survives every retry | 3 |
| `prop_both_databases_converge_byte_identical` | **the chaos property** | 3 |
| `prop_balance_equals_ledger_sum` | loyalty is conflict-free | 3 |
| `prop_promotions_never_increase_total` | a promotion that costs the customer money is a bug | 4 |
| `prop_promotion_proration_conserves_to_the_fil` | campaign cost reporting is trustworthy | 4 |
| `prop_wac_never_negative` | inventory valuation stays sane | 4 |

---

## IPC contract tests

From [`ipc-contract.md`](ipc-contract.md) §5 — the five rules the command catalog encodes:

| Test | Rule |
|---|---|
| `ipc_commands_all_declare_a_capability` | a command without a capability breaks CI |
| `ui_never_computes_money` | every displayed total traces to a `CartSnapshot` field |
| `no_command_issues_a_card_auth_outside_the_flow` | the UI cannot double-charge |
| `expected_is_not_sent_to_the_ui_before_the_count_is_submitted` | blind close is a wire guarantee |
| `no_command_mutates_a_completed_sale` | not a disabled one — none exists |

---

## Golden files

| Golden | Ph | Proves |
|---|---|---|
| `receipt_ar_80mm.bin` | 1 | Arabic shaping, RTL order, alignment |
| `receipt_ar_58mm.bin` | 1 | narrow profile reflows |
| `receipt_bilingual_80mm.bin` | 1 | mixed runs in one line |
| `receipt_multirate_80mm.bin` | 1 | exempt and standard as distinct summary rows |
| `receipt_duplicate_80mm.bin` | 1 | DUPLICATE watermark |
| `receipt_training_80mm.bin` | 1 | TRAINING watermark |
| `fiscal_plain.xml` | 2 | the baseline document |
| `fiscal_discounted.xml` | 2 | per-line percentage discounts (C-2) |
| `fiscal_multirate.xml` | 2 | multiple tax categories on one invoice |
| `fiscal_weighed.xml` | 2 | fractional quantity, UoM code |
| `fiscal_credit_note.xml` | 2 | the reversal, referencing its original |
| `label_ar.bin` | 4 | shelf label with price and barcode |
| `canonical_audit_entry.json` | 1 | the hash chain's serialization is byte-stable |

---

## Drills — manual, documented, timed

Not tests. Procedures, performed on real hardware, written down, and repeated by someone who did not write the code.

| Drill | Ph | Proves |
|---|---|---|
| Card reconciliation | 2 | tenders match the PSP ledger by `psp_ref`, to the fil |
| Blind-Z | 2 | a scripted day with drops and paid-outs balances to zero |
| Hardware lab | 2, then every release | Arabic on paper, confirmed by a native reader |
| Restore — data loss | 5 | unsynced sales survive a wipe; **the time is the merchant's downtime promise** |
| Restore — keychain loss | 5 | E.4 end to end |
| Fiscal certification | 5 | eleven items, dated and signed — the only thing that makes "JoFotara compliant" true |
| Three-store pilot week | 4 | the product survives people |

---

## Coverage summary

| Status | Count | Cases |
|---|---|---|
| ✅ Tested | 62 | all except those below |
| ⚠️ Accepted risk, disclosed | 4 | 31, 55, 61 *(only when enabled)*, 63 |
| 🧩 Deferred with a named hook | 4 | 45, 64, 66, 68 |
| 🚫 Out of v1 scope with a rationale | 2 | 67, 72 |

62 + 4 + 4 + 2 = 72. The deferred row previously said 3 while naming four cases, which made the
column sum to 73 against a total of 72 — corrected here rather than left for the reader to
reconcile. Cases 1b, 4b and 41b are *variants* of cases 1, 4 and 41, not additional numbered cases.

**Every one of the 72 has a row.** That is what "comprehensive" means operationally: not that nothing exists beyond this list, but that everything on it has a deliberate status.

When the three-store pilot (Phase 4) or a merchant surfaces something new, it becomes E.73 here — with a test, an accepted risk, or an out-of-scope. **A surprise that becomes none of the three will happen again.**
