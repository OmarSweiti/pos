# Test catalog — the coverage matrix

Master plan Parts E and J.4 enumerate 72 things that can happen. The plan audit and the phase work added twenty more. This file maps **every one** to a named test, an accepted risk with a written rationale, an open question with a stated default, or an explicit out-of-scope with a reason.

**The rule (master plan J.0, applied to itself):** an edge case with no row here is a gap, not an absence. A row with no test and no rationale is an unfinished job.

Legend — **Ph**: phase the test lands in · **Kind**: `unit` · `prop` · `golden` · `integration` · `chaos` · `fuzz` · `web` (a DOM component or browser test) · `drill` (a manual, documented, timed procedure).

---

## How this file stays honest

Nothing mechanically reconciled this matrix against the test suite, so the Phase-5 exit gate — *"every one of E.1–E.72 is a passing test, an accepted risk, or out of scope"* — reduced to reading a hand-maintained table. It had already drifted before Phase 1 began: two named golden tests did not match the microstep that builds them, a case claimed at Phase 2 lived at Phase 4, three phases each claimed E.31, and four rows named tests that no microstep asks anyone to write.

**`scripts/check-test-catalog.py`** closes that. Local `just lint` reconciles the real catalog, and `just guards` runs its `--self-test`; both are deliberately absent from `ci.yml` until the frozen workflow surface receives a separate reviewed edit, as [`03-github-workflow.md`](../03-github-workflow.md) §3 records. Seven assertions are mechanical:

1. **Every backtick-quoted exact `snake_case` identifier in a Test column is a test name and exists in its runner's listing or in `PLANNED`** — `cargo nextest list --workspace --message-format json` for the Rust kinds, the `vitest` listing for `web`, the fuzz-target directory for `fuzz`. Schema fields, mock faults and other incidental identifiers stay plain text in this column so prose cannot masquerade as evidence. Pure `drill` rows and rows marked ⚠️ accepted risk, 🧩 deferred or 🚫 out of scope are exempt because their evidence is a dated procedure, a disclosed rationale or a named hook rather than a test binary; ⏳ and ❓ rows are not exempt.
2. **Every entry in `PLANNED` names the microstep that will build it**, and the test appears on that microstep's `Tests:` line. An entry with no microstep number, a microstep that does not exist, or a catalogue phase that excludes its owner is a failure.
3. **`PLANNED` may only shrink.** A growing allowlist fails the check, which is what stops "planned" from becoming a synonym for "never".
4. **Every `E.n` cited in a phase file's `Tests:` line or exit gate has a row here whose `Ph` column includes that phase.** This is the assertion that would have caught a Phase-1 gate demanding a Phase-3 case.
5. **Numbered cases are contiguous and unique** — every number from 1 to the highest has exactly one row. A case spanning two phases says so in its `Ph` column; it does not get a second row, because a second row is how one case comes to be counted twice.
6. **The coverage summary is recomputed from the rows**, by reading each row's status marker. A typed total that disagrees fails. This file once carried a column that summed to 73 against a total of 72; assertion 6 is why that cannot recur — and it is why every non-tested row carries a marker rather than only a sentence.
7. **Every normative test or property name in `ref/*.md` has exactly one owning phase-microstep `Tests:` line.** An alias, ownerless promise or competing owner fails. Catalog Test-column names remain subject to the stronger runner/`PLANNED` checks above.

`--self-test` proves all seven assertions, including a clean normative name, an orphan and a competing owner alias.

---

## Power, crash, and state

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 1 | Power cut mid-`Finalizing` | `interrupted_finalize_resumes_without_double_stock_event`, `interrupted_finalize_resumes_without_double_outbox_row`, `finalize_is_atomic_under_injected_failure` | 1 | integration |
| 1b | **Real** power loss after the receipt printed | **post-receipt power-cut drill**, on register hardware, with the power actually cut. Case 1's `pkill -9` cannot test this: killing the process leaves the OS page cache alive, so the writes land whether synchronous is NORMAL or FULL. Enforced by synchronous = FULL **in WAL**; `pos_db::open` asserts both and refuses otherwise (`the_register_runs_in_wal_mode`, `a_register_database_commits_durably`) | 1 | drill |
| 2 | Power cut during card `Tendering` | `unknown_triggers_status_query_before_any_retry`, `status_query_approved_attaches_tender` | 2 | unit |
| 2b | Card **approved**, then power cut before the sale commits — the money moved and no document exists | `an_interrupted_tendering_is_recovered_and_status_queried`, `a_checkout_operation_row_never_outlives_its_commit`. Recovery needs durable in-flight state; the protocol unit tests in case 2 assert the state machine, not the restart | 1 / 2 | integration |
| 3 | App killed with parked carts | `prop_park_resume_roundtrip_is_identity`, `parked_carts_survive_restart` | 1 | prop |
| 4 | SQLite `BadKey` — keychain wiped | `bad_key_yields_recovery_state_not_panic` + **keychain-loss restore drill** | 1 / 5 | unit + drill |
| 4b | A fact table is left writable — the audit trail, stock ledger or cash trail edited after the fact | `every_fact_table_refuses_the_write_that_would_rewrite_history`, `the_frozen_row_table_covers_every_declared_fact_table` | 1 | integration |
| 4c | A migration rebuilds the tables holding every completed sale | `the_rebuild_keeps_every_row_of_a_completed_sale`, `the_rebuilt_tables_are_all_strict`, `the_rebuild_restores_the_immutability_triggers`, `no_staging_table_survives_the_rebuild`, `after_the_rebuild_the_six_tables_enforce_their_types` | 1 | integration |
| 4d | The credential store **and** the machine are destroyed together — the case case 4 does not cover, because both copies of the key were the same copy | `a_backup_opens_with_the_recovery_code_alone`, `key_generation_refuses_when_a_database_already_exists`, `the_wrapped_envelope_travels_with_every_backup` + **recovery-code restore drill** on a second machine | 1 / 5 | integration + drill |
| 5 | Disk full | `low_disk_blocks_new_sales_and_alarms` | 1 | integration |
| 6 | Clock skew / cashier changes system time | `prop_monotonic_clock_never_decreases`, `clock_jump_back_reports_anomaly` | 1 | prop |
| 7 | DST / timezone; Z day boundary belongs to the shift | `sale_at_0100_belongs_to_previous_business_date`, `z_belongs_to_the_shifts_business_date_not_the_wall_clock`, `business_date_survives_timezone_change` | 1 / 2 | unit |

## Offline and sync

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 8 | Offline for days | `deep_outbox_alarms_without_blocking_sales`, `offline_week_converges` | 3 | chaos |
| 9 | Product edited centrally while offline | `open_cart_keeps_captured_price_after_catalog_apply`, `finalized_sales_are_never_touched` | 3 | integration |
| 10 | Duplicate push after a network retry | `duplicate_batch_is_a_no_op`, `prop_apply_is_idempotent_under_any_replay_order` | 3 | prop |
| 11 | Partial batch failure / poison pill | `partial_failure_acks_per_commit`, `poison_commit_goes_to_dead_letter_without_blocking`, `an_incomplete_commit_group_is_held_not_partially_applied` | 3 | integration |
| 12 | Two registers sell the last unit offline | `two_offline_registers_selling_the_last_unit_both_succeed`, `both_sales_of_the_last_unit_stand_and_stock_goes_negative_flagged` | 1 / 3 | chaos |
| 13 | Register clone / restore from image | `a_cloned_image_fails_its_first_authenticated_request`, `device_id_collision_refuses_sync_with_a_named_error`. Detection at *registration* cannot see this threat: a cloned disk already holds the original register id and token, so it never registers again | 3 | integration |
| 88 | A register two versions behind pushes to a newer server, mid staged rollout | `an_unsupported_protocol_version_fails_the_batch_and_applies_nothing`, `a_version_mismatch_never_dead_letters_a_fact`, `a_too_old_register_keeps_selling_and_says_so_in_device_health` | 3 | integration |
| 89 | The same UUID arrives twice carrying **different** payloads | `an_identical_replay_is_reported_as_duplicate`, `a_different_payload_under_a_known_uuid_is_rejected_and_alarms`, `the_stored_row_is_never_mutated_by_a_conflict` | 3 | integration |

## Money and rounding

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 14 | Split cash+card with cash rounding | `prop_cash_rounding_only_on_final_cash_tender`, `card_charged_exact_unrounded_total` | 1 | prop |
| 15 | Partial card approval | `partial_approval_leaves_remaining_due`, `partial_then_abandon_reverses`, `reversal_failure_escalates_and_audits` | 2 | unit |
| 16 | Refund exceeding remaining refundable | `prop_cumulative_refunds_never_exceed_sold_qty` | 2 | prop |
| 17 | Change due but drawer lacks denominations | `paid_in_from_safe_adjusts_expected_cash` — correctness is unaffected; the count helper is the UX answer | 2 | unit |
| 18 | 0.000 JOD total (100% discount / full redemption) | `prop_zero_total_cart_is_valid`, `zero_due_tender_completes_and_issues_a_fiscal_doc` | 1 / 2 | prop |
| 19 | Negative-price line attempts | `prop_discount_never_makes_a_line_negative`, conformance rule `F-010` | 1 / 2 | prop |
| 19b | **The same basket scanned in a different order.** Largest-remainder proration awards the leftover fil by position, so on a multi-rate basket reordering the lines moves a fil between a taxed line and an exempt one — and changes the total the customer pays | `prop_price_cart_is_invariant_under_line_reordering` | 1 | prop |
| 73 | Cash refund of a cash-rounded sale — the derived amount is not a multiple of the coin step and cannot be handed over | `cash_refund_is_rounded_to_the_coin_step`, `prop_refund_rounding_keeps_expected_cash_exact` | 2 | unit |
| 75 | Full refund of a line that carried a prorated basket discount | `prop_refunding_every_unit_returns_the_line_total_exactly`, `prop_partial_refunds_sum_to_the_line_total`, conformance rule for a credit note netting its invoice to zero | 2 | prop |
| 77 | A bank deposit leaves the **safe**, not the drawer, while a shift is open | `a_safe_to_bank_movement_does_not_change_expected_drawer_cash`, `every_movement_kind_has_a_term_in_expected_cash` — an exhaustive match over the `cash_movement.kind` list, so a seventh kind cannot ship without a term | 2 | unit |
| 79 | The counted cash at close is left in the drawer as the next shift's float | `a_carried_float_is_declared_once_and_reconciles_across_both_shifts` | 2 | unit |

## Card terminal

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 20 | Timeout after the customer tapped | `prop_no_input_sequence_yields_two_tenders_for_one_auth` | 2 | prop |
| 21 | Terminal offline / unpaired at Pay | `card_disabled_when_terminal_unreachable_cash_still_works` | 2 | unit |
| 22 | Refund to an expired/cancelled card | `refund_api_error_offers_store_credit_with_manager_approval` | 2 | unit |
| 23 | Settlement mismatch vs. PSP report | `settlement_report_lists_unmatched_separately_by_direction` + **card reconciliation drill** | 2 | integration + drill |

## Fiscal (JoFotara)

*Every row in this section is tested against the reconstruction in [`fiscal-jofotara.md`](fiscal-jofotara.md) §3, and stays **provisional** until microstep 2.7.0 pins the official specification. A green harness proves the pinned package was implemented consistently. It does not prove ISTD accepts anything.*

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 24 | API or ICV allocator down at sale time | mock connection-refused fault; `backoff_has_jitter`, `a_sale_completes_with_a_null_icv_and_allocates_on_reconnect`, `a_store_scoped_counter_allocates_in_order_on_reconnect`, `prop_icv_is_gap_free_and_strictly_increasing_within_its_scope` | 2 | integration + prop |
| 25 | Validation rejection | mock fault `400`; `rejection_dead_letters_verbatim_and_never_mutates_the_sale` | 2 | integration |
| 26 | Refund of a not-yet-cleared sale | `prop_credit_note_never_precedes_its_invoice` | 2 | prop |
| 27 | Duplicate submission after ambiguous timeout | `an_ambiguous_timeout_resends_identical_bytes_under_the_same_uuid`, `duplicate_recovery_follows_the_pinned_procedure`. The previously assumed `409`-plus-fetch contract is not in any official document and is no longer asserted | 2 | integration |
| 28 | Wrong environment credentials | `production_build_refuses_mock_credentials`, `tin_mismatch_in_response_alarms` | 2 | unit |
| 29 | Merchant not in a mandatory wave | `disabled_profile_produces_no_queue_row`, `receipt_prints_without_qr_when_disabled` | 2 | unit |
| 87 | **Two registers under one taxpayer, both offline, both issuing fiscal documents.** The counter was per register, so each allocated `1` | `two_offline_registers_never_allocate_the_same_icv` + the Phase-3 server-lease reconnect fixture | 3 | integration |
| | | ❓ **Open.** The tests above encode a store-scoped counter as the default. See the block below | | |
| 92 | A document fails the **local** pre-submit check — neither an ISTD rejection nor an exhausted retry | `a_build_failure_becomes_build_failed_and_never_rejected`, `build_failed_is_excluded_from_dead_letter_count`, `a_rebuild_preserves_the_uuid_and_any_allocated_icv` | 2 | unit |

> ⚠️ **OPEN — blocks 2.7.0.** Is the authoritative ICV namespace per register, store/income source, or one TIN across stores? Default until answered: allocate from one store-scoped counter keyed as `('store', store_id, 'fiscal_icv')`; Phase 2 uses the single register's in-process allocator, Phase 3 uses a server-issued one-value lease, and no register advances an independent register-scoped ICV counter.
> Owner: 2.7.0. Source that settles it: the official ISTD business rules or a written ISTD E-Invoicing Directorate ruling.

## Returns and fraud

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 30 | Return of an exchanged item | `refund_of_an_exchanged_item_follows_the_chain` | 2 | integration |
| 31 | Serial refund abuse across two stores | `serial_refund_attempt_is_caught_when_connected_and_surfaced_when_not` (detection, Phase 3) · `second_store_refund_of_the_same_receipt_is_refused_when_connected` (enforcement, Phase 4) | 3 / 4 | chaos + integration |
| | | ⚠️ **Accepted risk:** inside an offline window both can succeed. Mitigated by the connected check and the refunds-by-user report. **Disclosed to the merchant.** Phase 2 cannot hold this case at all — there is no server and no second store until Phase 3 | | |
| 32 | Receiptless return of a never-sold item | `receiptless_denied_when_disabled`, `receiptless_respects_threshold_and_requires_manager` | 2 | unit |
| 33 | Price-override abuse | `override_report_groups_by_user_with_reasons`, `override_below_floor_is_denied`, `no_command_argument_carries_a_price` | 1 / 4 | unit |
| 34 | Refund after a price change | `refund_uses_original_price_after_a_price_change`, `prop_refund_uses_original_rate` | 2 | prop |
| 35 | No-sale drawer-open spikes | `no_sale_open_is_logged_and_counted`, `z_report_counts_no_sale_opens`. Only **software-commanded** opens are observable; a manual key or a suppressed cash sale is not, and that residual is stated in [`security-compliance.md`](security-compliance.md) §9 | 2 | unit |
| 74 | Customer returns one unit of a "3 for 1.000" group | `partial_return_of_a_multibuy_reprices_the_remainder`, `prop_refund_never_leaves_the_customer_better_off_than_not_buying`. The policy is `DealBreak` by default: the kept quantity is repriced at its un-promoted price and the difference refunded | 2 / 4 | unit |
| 76 | Two cashiers transact on one drawer across a user switch; the close is short | `user_switch_inside_an_open_shift_is_refused_when_the_policy_forbids_it`, `over_short_is_attributed_to_the_shift_and_its_opener_not_invented_per_cashier` | 2 | unit |
| 81 | Exchange where the new item is cheaper and the original was paid by card | `prop_exchange_pair_nets_to_the_customer_facing_difference`, `exchange_with_a_negative_difference_routes_to_the_original_card`, `an_exchange_tender_is_never_cash_counted` | 2 | unit |
| 82 | Defective item returned on day 30 against a 14-day window | `defective_claim_bypasses_the_window_with_manager_approval`, `change_of_mind_outside_the_window_is_still_refused`, `a_defective_refund_records_the_reason_code` | 2 | unit |
| 84 | A shift lead runs an X report two minutes before their own blind count | `x_report_does_not_reveal_expected_cash_to_the_closing_user` | 2 | integration |

## Catalog and pricing

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 36 | Barcode collision | `a_second_live_barcode_claim_is_refused`, `barcode_conflict_report_lists_both` | 1 / 4 | unit |
| 37 | Price changed while an item is in an open cart | `open_cart_keeps_captured_price_after_catalog_apply`, `reprice_cart_action_applies_new_prices` | 3 | integration |
| 38 | Product deactivated with stock remaining | `inactive_product_cannot_be_added_but_can_be_refunded` | 1 | unit |
| 39 | Unknown barcode scan | `unknown_barcode_offers_quick_add_or_department_sale`, `queue_never_stalls_on_unknown_code` | 1 | unit |
| 39b | A search query containing FTS5 operator characters — `"`, `(`, `)`, `:`, `*`, a bare `OR`. The query string is interpolated into a `MATCH` expression, which parses it as an expression even when it is bound as a parameter | `search_survives_every_fts5_metacharacter`, `prop_no_query_string_produces_a_database_error`. Search is the only path a cashier has when the scanner fails, so an exception here stalls the queue | 1 | prop |
| 40 | Price-embedded barcode with a checksum error | `prop_corrupt_digit_never_parses_clean` | 1 | prop |
| 41 | Unicode names (Arabic + emoji) | `golden_receipt_ar_80mm`, `unicode_names_roundtrip_through_db_and_fts` | 1 | golden |
| 41b | Arabic typed in a different but equivalent spelling — tashkeel, أ/إ/آ for ا, ى for ي, ة for ه, tatweel | `fts_matches_arabic_with_and_without_diacritics`, `fts_matches_alef_and_yaa_spelling_variants`, `fts_matches_taa_marbuta_spelled_as_haa`, `fts_ignores_tatweel`, `fts_prefix_search_works_at_two_characters`, `exact_spelling_outranks_a_folded_variant`, `a_single_letter_query_does_not_match_a_vocalised_name` | 1 | integration |
| 78 | A product carries both a single-unit and a 6-pack barcode | `a_multipack_barcode_adds_its_pack_quantity`, `a_pack_quantity_of_zero_is_refused_at_save`. Both documents named multipacks as *the reason* a product has several barcodes, and the table carried no quantity — so an outer case of cola scanned at one can's price | 1 | unit |
| 80 | A deli label printed at 09:00 with an embedded price is scanned at 21:00, after the price per kg changed | `price_embedded_line_total_equals_the_label`, `price_embedded_stock_event_carries_the_derived_weight_flagged_estimated` | 1 | unit |
| 83 | Unknown barcode at 22:00 with no manager on site | `a_cashier_has_a_path_forward_without_product_edit`, `a_department_sale_carries_its_own_tax_category_and_audits`. The default policy required a capability the cashier does not hold, against a stated five-second rule | 1 | unit |

## Inventory

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 42 | Count during trading | `count_tolerates_sales_mid_count` | 4 | integration |
| 43 | Receiving with a fat-fingered cost | `ten_times_cost_requires_confirmation`, `corrective_adjust_recomputes_wac` | 4 | unit |
| 44 | Transfer arrives short or damaged | `short_receipt_creates_destination_adjustment_and_notifies` | 4 | integration |
| 45 | Expiry-dated goods | 🧩 **Deferred.** Lot tracking is a later module; v1 covers it with expiry-waste adjustments by reason. `waste_adjustment_by_reason_code` is the compensating test owned by 4.3.5 | 4 | unit |

## Documents and hardware

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 46 | Paper out mid-receipt | `print_failure_after_finalize_leaves_sale_complete`, `reprint_is_byte_identical_including_qr`, `an_unknown_print_outcome_never_auto_retries_the_drawer_pulse` | 1 / 2 | integration |
| 47 | Reprint days later from another register | `another_register_fetches_the_reprint_bundle_when_connected`, `document_fetch_is_refused_offline_with_a_named_error`, `a_fetched_bundle_is_never_written_to_the_local_database`. Facts travel up only, so the QR alone never made this work — it is an on-demand fetch, not replication ([`sync-protocol.md`](sync-protocol.md) §3) | 3 | integration |
| 48 | Email receipt bounce | `email_bounce_logged_without_retry_storm` — receipt remains printable | 3 | unit |
| 49 | 58 mm printer at a kiosk | `golden_receipt_ar_58mm`, `narrow_profile_reflows_rather_than_truncates` | 1 | golden |
| 50 | Drawer jammed/open at shift close | `jammed_drawer_does_not_block_shift_close` | 2 | unit |
| 85 | **No printer at all** — unplugged at shift open, or dead at 09:00 on a Saturday. The set handled paper running out mid-receipt and never handled the printer being absent | `a_sale_completes_with_no_printer_and_queues_the_artifact`, `the_missing_printer_is_an_alarm_not_a_modal`, `a_queued_artifact_prints_unchanged_once_a_printer_returns` | 1 | integration |
| | | ❓ **Open.** The tests encode the default below | | |

> ⚠️ **OPEN — blocks 2.7.0.** May a fiscally-enabled store trade when it cannot print, given that the cleared QR must appear on the document handed to the customer? Default until answered: the sale completes, the artifact is persisted and queued, and the register raises an alarm — never a refusal to sell, because a printer fault is not a reason to close a shop. Owner: 2.7.0, with the merchant-facing choice recorded in [`merchant-decisions.md`](merchant-decisions.md). Source that settles it: the official ISTD guidance on document issuance, and the merchant's tax advisor in writing.

## People and access

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 51 | Cashier forgets PIN | `manager_reset_retires_old_hash_and_audits` | 1 | unit |
| 52 | Manager approving their own escalation | `an_actor_cannot_approve_their_own_handle` | 1 | integration |
| 53 | Shift left open overnight | `stale_shift_detected_and_force_closed_with_flag` | 2 | integration |
| 54 | Training mode left on | `training_sales_are_excluded_with_a_visible_count`, `training_sale_produces_no_document`, `training_auto_off_at_shift_close` | 1 / 2 | unit |
| 55 | Terminated employee's PIN | `deactivation_applies_at_next_contact`, `offline_auth_window_expires_and_says_why`, `an_expired_lease_refuses_a_refund_and_says_why`, `selling_continues_while_authority_is_suspended` | 3 | integration |
| | | ⚠️ **Accepted risk:** the PIN works until next contact or lease expiry. **A real limit of offline-first; disclosed.** | | |
| 86 | A manager approval is replayed against a second, larger refund | `a_handle_used_twice_is_refused`, `an_altered_amount_is_refused`, `a_different_sale_is_refused`, `a_different_actor_is_refused`, `a_consumed_handle_is_still_consumed_after_restart`, `an_expired_handle_is_refused`, `the_effect_and_the_consumption_commit_together_or_not_at_all` | 1 | unit |
| 90 | One merchant's back-office session reads another merchant's data | `prop_no_query_crosses_an_org_boundary`, `rls_is_forced_on_every_merchant_owned_table`, `a_composite_foreign_key_refuses_a_cross_org_parent`, `two_orgs_may_use_the_same_sku`, `http_routes_all_declare_a_capability` | 3 | integration |
| 91 | The newest audit rows are deleted — the one tamper a local hash chain cannot see | `tail_deletion_is_detected_against_the_last_anchor`, `a_z_close_anchors_the_head`, `tail_deletion_is_detected_against_the_server_checkpoint`, `a_forked_checkpoint_is_refused_and_alarms`, `mutating_an_identity_column_breaks_the_chain` | 1 / 2 / 3 | integration |

## Platform and lifecycle

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 56 | Auto-update mid-shift | `update_deferred_while_shift_open`, `a_failed_update_before_migration_restores_the_previous_bundle`, `a_post_migration_failure_restores_the_pre_update_snapshot_or_rolls_forward`, `webview_cannot_invoke_the_updater_plugin`. "Rollback" cannot mean an older binary against a migrated database: the runtime refuses it by design with `SchemaTooNew`, which is correct and is why rollback is a snapshot restore, not a downgrade | 5 | integration |
| 57 | Licence expiry offline | `licence_expiry_never_prevents_a_sale_on_an_entitled_register`, `expiry_blocks_enrollment_and_updates`, `grace_period_survives_a_long_outage`, `an_entitlement_for_another_org_is_rejected` | 3 | unit |
| 58 | Migration failure on update | `half_migrated_db_refuses_to_open_with_a_named_error` (1.8.1b), `all_migrations_run_against_soak_dataset_within_budget` (5.5.3, against its 60-second budget) | 1 / 5 | integration |
| 59 | Telemetry offline | `offline_telemetry_is_buffered_and_capped`, `no_pii_in_a_captured_panic` | 3 | unit |
| 60 | Multi-monitor / resolution chaos | `sale_screen_min_size_guard`, kiosk fullscreen mode | 1 | unit |

## J.4 additions — stored value, wallets, age, price control

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 61 | Gift card sold offline, redeemed elsewhere before sync | `stored_value_is_online_authorize_only_by_default` | 2 / 4 | unit |
| | | ⚠️ **Accepted risk when the offline cap is deliberately enabled.** Quantified in settings; off by default | | |
| 62 | Store credit issued offline at two stores | `two_offline_registers_cannot_both_spend_the_same_balance` | 2 | integration |
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

Every row names a `Strategy`, and the strategy lives beside the property with a comment saying what input space it covers **and what it deliberately excludes** — an unstated generator is the difference between a property that bites and a green tick. Case counts, seed persistence and the ban on wall-clock assertions inside `proptest!` are engineering law, in [`01-conventions.md`](../01-conventions.md) §5.

| Property | Guards | Ph |
|---|---|---|
| `prop_split_preserves_total` *(exists)* | splitting a tender never changes the total | 0 |
| `prop_split_proportional_preserves_total` *(exists)* | proration conserves to the fil, for any weights | 1 |
| `prop_currency_mismatch_never_silently_coerces` *(exists)* | a JOD amount can never become a USD one | 1 |
| `prop_sql_and_rust_folding_agree` | the fold in the 0003 generated column and the fold the query path applies produce the same string, for any input — two implementations of one rule, so they will drift unless something holds them together | 1 |
| `prop_no_query_string_produces_a_database_error` | a cashier's keystrokes reach an FTS5 `MATCH` expression; none of them may throw | 1 |
| `prop_inclusive_net_plus_tax_equals_gross` | inclusive extraction is exact, at every rate | 1 |
| `prop_line_tax_sum_equals_receipt_tax` | the summary is the sum, never a re-derivation | 1 |
| `prop_exempt_and_zero_produce_zero_tax_but_differ_in_reporting` | the distinction survives to the filing report | 1 |
| `prop_total_equals_lines_minus_discounts_plus_tax` | the master identity | 1 |
| `prop_no_operation_mutates_a_complete_sale` | immutability, by construction | 1 |
| `prop_basket_discount_prorates_to_the_fil` | no fil is created or destroyed | 1 |
| `prop_price_cart_is_invariant_under_line_reordering` | the same basket costs the same whatever order it was scanned in — the fil that largest-remainder awards by position must not move tax between a taxed line and an exempt one | 1 |
| `prop_discount_never_makes_a_line_negative` | ISTD rejects negative lines; so do we | 1 |
| `prop_cash_rounding_only_on_final_cash_tender` | card pays exact; cash rounds once | 1 |
| `prop_rounding_adjustment_keeps_total_exact` | the books still reconcile | 1 |
| `prop_refund_rounding_keeps_expected_cash_exact` | a rounded payout leaves the drawer reconcilable | 2 |
| `prop_cache_rebuild_matches_ledger` | the on-hand cache can never silently diverge | 1 |
| `prop_chain_detects_any_single_entry_mutation` | tamper-evidence actually evidences | 1 |
| `prop_chain_detects_deletion_before_the_anchor` | removing any entry protected by the retained anchor breaks the chain. Tail deletion above the newest anchor remains outside the claim | 1 |
| `prop_monotonic_clock_never_decreases` | a changed system clock cannot reorder history | 1 |
| `prop_cumulative_refunds_never_exceed_sold_qty` | the anti-abuse core, in any order | 2 |
| `prop_refunding_every_unit_returns_the_line_total_exactly` | a full refund returns what the customer paid, to the fil, on a discounted line | 2 |
| `prop_partial_refunds_sum_to_the_line_total` | the last partial refund absorbs the remainder rather than destroying it | 2 |
| `prop_refund_never_leaves_the_customer_better_off_than_not_buying` | returning part of a multibuy cannot pay out more than the deal was worth | 2 |
| `prop_exchange_pair_nets_to_the_customer_facing_difference` | two linked documents, one difference settled | 2 |
| `prop_no_input_sequence_yields_two_tenders_for_one_auth` | **no double charge, ever** | 2 |
| `prop_expected_cash_matches_movement_replay` | the drawer reconciles from first principles | 2 |
| `prop_expected_cash_equals_physical_drawer_replay` | simulate the coins in and out and the formula reproduces the count — order-independence is not correctness | 2 |
| `prop_z_number_is_gap_free` | an auditor's first question | 2 |
| `prop_document_allowance_recap_equals_sum_of_line_allowances` | the fiscal document's discount recap is the sum of its line allowances, exactly. Replaces `prop_discount_percentage_roundtrip_is_exact`: a percentage round-trip is not a representation of the money, and gating submission on it dead-lettered arithmetically correct baskets | 2 |
| `prop_line_level_drift_never_exceeds_half_a_fil` | per-line rounding drift is bounded by construction, so the pre-submit check cannot reject a correct document | 2 |
| `prop_credit_note_never_precedes_its_invoice` | fiscal dependency ordering | 2 |
| `prop_icv_is_gap_free_and_strictly_increasing_within_its_scope` | whatever the scope turns out to be, two registers cannot both allocate `1` | 2 |
| `prop_apply_is_idempotent_under_any_replay_order` | sync survives every retry | 3 |
| `prop_server_facts_equal_the_union_of_register_outboxes` | every fact each register produced is on the server exactly once, with identical canonical bytes, and nothing else is | 3 |
| `prop_reference_tables_converge_across_all_three_nodes` | a caught-up register's reference state projects identically to the server's. Replaces `prop_both_databases_converge_byte_identical`, which could not hold: facts travel up only, the two engines store types differently, and caches are local by design | 3 |
| `prop_consent_effective_state_is_the_latest_accepted_event` | a stale grant can never overwrite a withdrawal | 3 |
| `prop_no_query_crosses_an_org_boundary` | two fully populated orgs; every read and write attempted as each | 3 |
| `prop_balance_equals_ledger_sum` | loyalty is conflict-free | 3 |
| `prop_promotions_never_increase_total` | a promotion that costs the customer money is a bug | 4 |
| `prop_promotion_proration_conserves_to_the_fil` | campaign cost reporting is trustworthy | 4 |
| `prop_wac_never_negative` | inventory valuation stays sane | 4 |
| `prop_wac_is_between_the_min_and_max_cost_ever_received` | the constraint that actually catches a blended phantom cost; "never negative" passes for any nonsense above zero | 4 |

---

## IPC contract tests

From [`ipc-contract.md`](ipc-contract.md) §5 — the rules the command catalog encodes:

| Test | Rule |
|---|---|
| `ipc_commands_all_declare_a_capability` | a command without a capability breaks CI |
| `conditional_privilege_cannot_cross_threshold_without_approval` | a conditional branch cannot pass its threshold without a matching persisted handle |
| `sale_screen_renders_cart_total_and_status_strip` | exact fixture values prove every displayed total comes from `CartSnapshot` |
| `prop_no_input_sequence_yields_two_tenders_for_one_auth` | the UI cannot double-charge |
| `no_command_argument_carries_a_price` | `cart_add_line` took an optional `unit_price_minor` under the base sale capability, unaudited — a price override with no reason, no margin floor, no ceiling check and no audit row. The registry permits price fields only on audited `cart_override_price`, capped audited `cart_add_department_sale`, and inert content-hashed `product_quick_add_prepare`; every base-sale path is refused |
| `every_privileged_command_binds_its_approval` | every `Always` and `Conditional` entry binds the exact operation; a manager PIN never authorises a class of operations |
| `expected_is_not_sent_to_the_ui_before_the_count_is_submitted` | blind close is a wire guarantee |
| `x_report_does_not_reveal_expected_cash_to_the_closing_user` | a report cannot route around the blind-count wire guarantee |
| `prop_no_operation_mutates_a_complete_sale` | not a disabled mutation — no operation can reopen a completed sale |
| `the_effect_and_the_consumption_commit_together_or_not_at_all` | a crash cannot commit only the privileged effect or only its one-use consumption |
| `altering_a_stock_request_after_approval_is_refused` | every prepared stock field is covered by both the content-hash check and the database freeze |
| `altering_a_quick_add_request_after_approval_is_refused` | every prepared product field is covered by both the content-hash check and the database freeze |
| `fiscal_rebuild_failed_requires_bound_approval_and_preserves_identity` | the fiscal remediation command is bound to its queue row and cannot mint a replacement identity |
| `webview_cannot_invoke_the_updater_plugin` | the exhaustiveness test walks `generate_handler!` and separately proves `updater:default` cannot let a compromised webview install an update mid-checkout |
| `ipc_errors_carry_no_source_detail_in_release` | release IPC errors expose a stable code, never formatted source detail |

And the server's half, which the IPC registry does not cover because the back office is Axum, not Tauri ([`security-compliance.md`](security-compliance.md) §5a):

| Test | Rule |
|---|---|
| `http_routes_all_declare_a_capability` | a route with no registry entry breaks CI, exactly as an IPC command does |
| `an_unauthenticated_request_is_refused_on_every_route` | deny by default, asserted per route rather than assumed from middleware |
| `a_principal_without_a_store_grant_cannot_read_that_stores_sales` | scope comes from the principal, never from a path parameter |
| `mfa_is_required_for_every_privileged_capability` | the privileged set is a list, and the list is checked |
| `a_support_access_writes_an_audit_row_the_merchant_can_see` | a processor that cannot say who read what cannot answer its controller |

---

## Fuzz targets

Four parsers consume input this product does not control, and the panic ban (`unwrap`/`expect` denied outside tests) makes panic-freedom on hostile input an invariant rather than a nicety — a panic in a register is a lost sale. Property tests over *well-formed* inputs do not test this; a single-digit mutation of a valid barcode is not a 200-character scan.

| Target | Input | Asserts | Ph |
|---|---|---|---|
| `parse_scan` | arbitrary bytes, as a scanner emits them: truncated, overlong, non-numeric, hostile | never panics, never loops, never returns a clean parse for corrupt input | 1 |
| the receipt layout and raster entry point | arbitrary UTF-8 product names — RTL overrides, combining marks, unpaired surrogates in the source data, 10 000 characters | never panics; output never exceeds the printer profile's width | 1 |
| the UBL builder | arbitrary persisted-sale shapes | never panics; returns either a document or a named error | 2 |
| the sync decoder (`PushBatch`, `PullResponse`) | arbitrary JSON from the network | never panics; unknown fields, wrong versions and truncated bodies are named errors | 3 |

Seed corpora come from the Jordanian fixture and the golden set. A crash input is committed as a permanent regression the moment it is found. The fuzz job runs on a **weekly** schedule with a fixed time budget, never per pull request — a per-PR fuzzer is a flake source, and this repository has no flake policy on purpose.

---

## Harnesses the tests depend on

Several named tests cannot be written until something exists to write them against, and that something was nobody's microstep. A test whose harness is missing is discovered at the moment it is due, under time pressure, and gets demoted to a manual check.

| Harness | Unblocks |
|---|---|
| **Fault injection at the SQLite write boundary** — a seam that fails the *n*th write inside a transaction | `finalize_is_atomic_under_injected_failure` and every "power cut between two writes" row. Without it, case 1 is a `pkill` that cannot distinguish every variant of `FinalizeWritePoint::ALL` |
| **A DOM component harness for `apps/terminal`** — `@testing-library/react` + `user-event` + `jsdom`, with fake timers | `scan_routes_while_search_focused`, `scan_burst_detected_over_typing`, `every_action_reachable_without_a_mouse`, `latin_runs_inside_arabic_text_are_bidi_isolated`. `apps/terminal` has `vitest` and no DOM environment, while `apps/backoffice` already has the pattern |
| **A packaged-app WebDriver smoke suite** — `tauri-driver` + WebdriverIO | the only artefact a merchant runs is the one nothing currently executes: cold start, the capability file, the CSP, credential-store access per OS. Playwright drives browser engines, not a Tauri webview, so the `.spec.ts` naming is misleading |
| **Two registers plus a server, in-process** | the three convergence properties, the offline week, every dead-letter and conflict row |
| **A seeded chaos generator with committed seeds** | the same, reproducibly. A failing fault sequence that cannot be replayed is an anecdote |
| **The mock ISTD server with header-driven fault injection** | every fiscal row, and it is the only fiscal evidence available before 2.7.0 |
| **The simulated payment terminal with fault injection** | every card row, including the `Unknown` protocol |
| **A two-org fixture, both fully populated** | `prop_no_query_crosses_an_org_boundary`. One org proves nothing about isolation |
| **A soak dataset generator** | the migration budget, the performance budgets at year-one volume, and the outbox byte budget. It is cheaper to generate it at the end of Phase 2 than to discover an index in Phase 5 |

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
| `receipt_b2b_80mm.bin` | 1 | the buyer block — the one customer who explicitly asked for something |
| `fiscal_plain.xml` | 2 | the baseline document |
| `fiscal_discounted.xml` | 2 | line allowance amounts plus their document recap (C-2) |
| `fiscal_multirate.xml` | 2 | multiple tax categories on one invoice |
| `fiscal_weighed.xml` | 2 | fractional quantity, UoM code |
| `fiscal_credit_note.xml` | 2 | the reversal, carrying the original's immutable buyer and line facts |
| `label_ar.bin` | 4 | shelf label with price and barcode |
| `canonical_audit_entry.json` | 1 | the hash chain's serialization is byte-stable, identity columns included |

**A binary golden ships with a reviewable projection.** The five fiscal goldens are XML: a diff is readable, and "a change to any byte is visible in a diff" is earned. The receipt goldens are 1-bit rasters, where the same sentence is not true — a hexdump cannot show that an Arabic letter lost its medial form. So each `receipt_*.bin` and `label_*.bin` commits a `.png` beside it, generated by the same rasteriser and diffed in the same pull request, and a regenerated Arabic golden carries the native-reader confirmation **in that pull request** rather than deferred to the next release. Otherwise a `cosmic-text`, `rustybuzz`, `tiny-skia` or font bump produces a byte diff indistinguishable from a shaping regression, and `UPDATE_GOLDEN=1` is the only available response.

The fiscal goldens cannot be frozen before microstep 2.7.0 pins the specification.

---

## Drills — manual, documented, timed

Not tests. Procedures, performed on real hardware, written down, and repeated by someone who did not write the code.

**A drill produces a record or it did not happen.** Each run is a dated file — the drill, the commit or tag it ran against, the hardware, the operator's name, start and end time, elapsed, outcome, and any surprise plus the case number it became. "Signed off and dated" needs somewhere to be signed, and a normative reference document is not a log.

| Drill | Ph | Proves |
|---|---|---|
| Card reconciliation | 2 | tenders match the PSP ledger by `psp_ref`, to the fil |
| Blind-Z | 2 | a scripted day with drops, paid-outs and a safe movement balances to zero |
| Hardware lab | 2, then every release | Arabic on paper and on screen, confirmed by a native reader |
| Restore — data loss | 5 | unsynced sales survive a wipe; **the time is the merchant's downtime promise** |
| Restore — keychain loss | 5 | E.4 end to end |
| Restore — recovery code only | 5 | E.4d: the database and the credential-store entry are both destroyed, and the off-machine backup opens with the printed recovery code alone. This is the drill the old design could not pass |
| Key rotation | 5 | the database key, the entitlement key and the fiscal credentials each rotate without losing a queued document |
| Breach tabletop | 5 | both statutory clocks timed independently from discovery, and the containment sequence executed |
| Fiscal certification | 5 | nine credentialed items, dated and signed, after the 2.7.0 specification prerequisite — the only thing that makes "JoFotara compliant" true |
| Three-store pilot week | 4 | the product survives people |

---

## Coverage summary

| Status | Count | Cases |
|---|---|---|
| ✅ Tested — a named test with an owning microstep | 80 | all except those below |
| ⏳ Named test, no owning microstep yet | 0 | — |
| ⚠️ Accepted risk, disclosed | 4 | 31, 55, 61 *(only when enabled)*, 63 |
| ❓ Open — behaviour defaulted, pending an external answer | 2 | 85, 87 |
| 🧩 Deferred with a named hook | 4 | 45, 64, 66, 68 |
| 🚫 Out of v1 scope with a rationale | 2 | 67, 72 |

80 + 0 + 4 + 2 + 4 + 2 = 92. Cases 1b, 2b, 4b, 4c, 4d, 19b, 39b and 41b are *variants* of cases 1, 2, 4, 19, 39 and 41, not additional numbered cases.

An earlier revision of this table said 62 + 4 + 4 + 2 = 72 while the deferred column named four cases against a typed 3, summing to 73 — caught here rather than left for the reader. Assertion 6 of the checker above now recomputes every one of these numbers from the rows, so the next such error fails `just lint` instead of surviving a review.

**Two statuses are new and both are deliberate.** ⏳ replaces a ✅ that was not true: a case whose named test nobody is scheduled to write is not covered, and saying so is the only way it gets an owner. ❓ marks a case where the *behaviour* is implemented against a stated default while an external answer is outstanding — the greppable `⚠️ OPEN` blocks above carry the question, the default, the owning microstep, and the document that settles it.

**Every one of the 92 has a row.** That is what "comprehensive" means operationally: not that nothing exists beyond this list, but that everything on it has a deliberate status, and that a script rather than a reader checks it.

When the three-store pilot (Phase 4) or a merchant surfaces something new, it becomes E.93 here — with a test, an accepted risk, an open question with a default, or an out-of-scope. **A surprise that becomes none of those will happen again.**
