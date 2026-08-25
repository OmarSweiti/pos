#!/usr/bin/env python3
"""A named edge case is not coverage until its evidence can be found.

`ref/test-catalog.md` is the Phase-5 coverage gate. Before this check existed,
the catalogue, the phase that promised a test, and the runner that discovered
it could all disagree while every executable test stayed green. That is how a
hand-maintained matrix came to count 73 cases against a stated total of 72, how
one case was claimed by three phases, and how tests acquired names nobody was
scheduled to write.

The dangerous failure is not a red test. It is an absent test represented by a
green row. This checker therefore reconciles executable runner listings, the
shrinking PLANNED ceiling, phase-microstep `Tests:` ownership, and every
normative test list in the reference set. It also recomputes the case numbering
and status arithmetic, because arithmetic typed beside a table is not evidence
about that table.

PLANNED has a separately frozen ceiling from this checker's first reviewed
baseline. A landed or removed catalogue test moves from PLANNED to
PLANNED_RETIRED; the tombstone makes re-adding it fail. Adding a new name or
deleting one without a tombstone also fails assertion 3. Changing the ceiling
is therefore a visible policy change, not an invisible way to make "planned"
mean "eventually".

Usage:  ./scripts/check-test-catalog.py
        ./scripts/check-test-catalog.py --phase 1
        ./scripts/check-test-catalog.py --self-test
Exit:   0 clean · 1 a catalogue violation · 2 could not run at all
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "docs/implementation/ref/test-catalog.md"
REFERENCE_DIR = ROOT / "docs/implementation/ref"
PHASE_FILES = {
    phase: ROOT / f"docs/implementation/phase-{name}.md"
    for phase, name in {
        1: "1-sellable-mvp",
        2: "2-money-grade",
        3: "3-connected",
        4: "4-depth",
        5: "5-harden-and-launch",
    }.items()
}

SNAKE_CASE = re.compile(r"[a-z][a-z0-9]*(?:_[a-z0-9]+)*\Z")
CASE_ID = re.compile(r"(?P<number>[1-9][0-9]*)(?P<suffix>[a-z]?)\Z")
MICROSTEP = re.compile(r"[1-5]\.[0-9]+\.[0-9]+[a-z]?\Z")
MICROSTEP_HEADING = re.compile(r"^### (?P<step>[1-5]\.[0-9]+\.[0-9]+[a-z]?) — ")
E_REFERENCE = re.compile(
    r"(?<![A-Za-z0-9])E\.(?P<case>[1-9][0-9]*[a-z]?)(?![A-Za-z0-9])"
)
E_RANGE = re.compile(
    r"(?<![A-Za-z0-9])E\.(?P<start>[1-9][0-9]*)\s*[–—-]\s*"
    r"E\.(?P<end>[1-9][0-9]*)(?![A-Za-z0-9])"
)
CODE_SPAN = re.compile(r"`([^`\n]+)`")
FENCE_LINE = re.compile(r"^ {0,3}(?P<marker>`{3,}|~{3,})(?P<tail>.*)$")

STATUS_MARKERS = ("✅", "⏳", "⚠", "❓", "🧩", "🚫")
TESTED = "✅"
RUNNER_EXEMPT_STATUSES = frozenset({"⚠", "🧩", "🚫"})
RUST_KINDS = frozenset({"unit", "prop", "golden", "integration", "chaos"})
KNOWN_KINDS = RUST_KINDS | {"web", "fuzz", "drill"}

# This is an allowlist, not a second catalogue. Every name is absent from its
# runner today and is tied to the exact microstep whose Tests line promises it.
# As tests land, delete their entries; never add one merely to silence an
# absent-test failure.
PLANNED: dict[str, tuple[str, ...]] = {
    "1.1.9": (
        "clock_jump_back_reports_anomaly",
        "prop_monotonic_clock_never_decreases",
    ),
    "1.2.1": ("a_pack_quantity_of_zero_is_refused_at_save",),
    "1.2.3": (
        "a_multipack_barcode_adds_its_pack_quantity",
        "a_second_live_barcode_claim_is_refused",
        "prop_no_query_string_produces_a_database_error",
        "search_survives_every_fts5_metacharacter",
    ),
    "1.2.4": (
        "price_embedded_line_total_equals_the_label",
        "prop_corrupt_digit_never_parses_clean",
    ),
    "1.2.5": (
        "a_single_letter_query_does_not_match_a_vocalised_name",
        "exact_spelling_outranks_a_folded_variant",
        "fts_ignores_tatweel",
        "fts_matches_alef_and_yaa_spelling_variants",
        "fts_matches_arabic_with_and_without_diacritics",
        "fts_matches_taa_marbuta_spelled_as_haa",
        "fts_prefix_search_works_at_two_characters",
        "unicode_names_roundtrip_through_db_and_fts",
    ),
    "1.4.3": (
        "age_restricted_line_requires_confirmation",
        "inactive_product_cannot_be_added_but_can_be_refunded",
    ),
    "1.4.3b": ("age_decline_removes_line_and_audits",),
    "1.4.4": ("prop_park_resume_roundtrip_is_identity",),
    "1.4.5": ("prop_discount_never_makes_a_line_negative",),
    "1.4.7": (
        "displayed_price_override_queues_a_label_reprint",
        "override_below_floor_is_denied",
    ),
    "1.4.9": (
        "prop_price_cart_is_invariant_under_line_reordering",
        "prop_zero_total_cart_is_valid",
    ),
    "1.4.11": ("no_command_argument_carries_a_price",),
    "1.4.12": ("queue_never_stalls_on_unknown_code",),
    "1.5.3": (
        "card_charged_exact_unrounded_total",
        "prop_cash_rounding_only_on_final_cash_tender",
    ),
    "1.6.2": ("manager_reset_retires_old_hash_and_audits",),
    "1.6.4": (
        "a_consumed_handle_is_still_consumed_after_restart",
        "a_different_actor_is_refused",
        "a_different_sale_is_refused",
        "a_handle_used_twice_is_refused",
        "an_altered_amount_is_refused",
        "an_expired_handle_is_refused",
        "the_effect_and_the_consumption_commit_together_or_not_at_all",
    ),
    "1.6.5": ("mutating_an_identity_column_breaks_the_chain",),
    "1.6.6b": ("tail_deletion_is_detected_against_the_last_anchor",),
    "1.7.3": ("narrow_profile_reflows_rather_than_truncates",),
    "1.7.5": (
        "golden_receipt_ar_58mm",
        "golden_receipt_ar_80mm",
    ),
    "1.7.6b": (
        "a_queued_artifact_prints_unchanged_once_a_printer_returns",
        "a_sale_completes_with_no_printer_and_queues_the_artifact",
        "the_missing_printer_is_an_alarm_not_a_modal",
    ),
    "1.7.7": (
        "an_unknown_print_outcome_never_auto_retries_the_drawer_pulse",
        "print_failure_after_finalize_leaves_sale_complete",
    ),
    "1.8.1b": ("half_migrated_db_refuses_to_open_with_a_named_error",),
    "1.8.2b": ("parked_carts_survive_restart",),
    "1.8.3": ("finalize_is_atomic_under_injected_failure",),
    "1.8.4": (
        "a_checkout_operation_row_never_outlives_its_commit",
        "an_interrupted_tendering_is_recovered_and_status_queried",
        "interrupted_finalize_resumes_without_double_outbox_row",
        "interrupted_finalize_resumes_without_double_stock_event",
    ),
    "1.8.5b": (
        "a_backup_opens_with_the_recovery_code_alone",
        "key_generation_refuses_when_a_database_already_exists",
        "the_wrapped_envelope_travels_with_every_backup",
    ),
    "1.8.7": ("bad_key_yields_recovery_state_not_panic",),
    "1.8.8": ("low_disk_blocks_new_sales_and_alarms",),
    "1.9.4": (
        "business_date_survives_timezone_change",
        "sale_at_0100_belongs_to_previous_business_date",
    ),
    "1.9.5": ("training_auto_off_at_shift_close",),
    "1.10.1": (
        "price_embedded_stock_event_carries_the_derived_weight_flagged_estimated",
    ),
    "1.10.4": ("two_offline_registers_selling_the_last_unit_both_succeed",),
    "1.11.10b": (
        "a_cashier_has_a_path_forward_without_product_edit",
        "a_department_sale_carries_its_own_tax_category_and_audits",
        "unknown_barcode_offers_quick_add_or_department_sale",
    ),
    "1.11.12": ("sale_screen_min_size_guard",),
    "1.12.5": ("training_sales_are_excluded_with_a_visible_count",),
    "2.1.3": (
        "prop_no_input_sequence_yields_two_tenders_for_one_auth",
        "status_query_approved_attaches_tender",
        "unknown_triggers_status_query_before_any_retry",
    ),
    "2.2.1": ("partial_approval_leaves_remaining_due",),
    "2.2.3": (
        "partial_then_abandon_reverses",
        "reversal_failure_escalates_and_audits",
    ),
    "2.2.4": ("card_disabled_when_terminal_unreachable_cash_still_works",),
    "2.2.6": (
        "pending_tender_is_never_silently_dropped",
        "pending_tender_polls_by_reference_before_declaring_unpaid",
    ),
    "2.3.2": (
        "a_defective_refund_records_the_reason_code",
        "change_of_mind_outside_the_window_is_still_refused",
        "defective_claim_bypasses_the_window_with_manager_approval",
        "partial_return_of_a_multibuy_reprices_the_remainder",
        "prop_cumulative_refunds_never_exceed_sold_qty",
        "prop_partial_refunds_sum_to_the_line_total",
        "prop_refund_never_leaves_the_customer_better_off_than_not_buying",
        "prop_refund_uses_original_rate",
        "prop_refunding_every_unit_returns_the_line_total_exactly",
        "refund_uses_original_price_after_a_price_change",
    ),
    "2.3.3": (
        "cash_refund_is_rounded_to_the_coin_step",
        "prop_refund_rounding_keeps_expected_cash_exact",
        "refund_api_error_offers_store_credit_with_manager_approval",
    ),
    "2.3.5": (
        "receiptless_denied_when_disabled",
        "receiptless_respects_threshold_and_requires_manager",
    ),
    "2.3.7": (
        "an_exchange_tender_is_never_cash_counted",
        "exchange_with_a_negative_difference_routes_to_the_original_card",
        "prop_exchange_pair_nets_to_the_customer_facing_difference",
        "refund_of_an_exchanged_item_follows_the_chain",
    ),
    "2.3.11": (
        "stored_value_is_online_authorize_only_by_default",
        "two_offline_registers_cannot_both_spend_the_same_balance",
    ),
    "2.4.2": (
        "user_switch_inside_an_open_shift_is_refused_when_the_policy_forbids_it",
    ),
    "2.4.3": ("a_safe_to_bank_movement_does_not_change_expected_drawer_cash",),
    "2.4.4": (
        "every_movement_kind_has_a_term_in_expected_cash",
        "paid_in_from_safe_adjusts_expected_cash",
    ),
    "2.4.7": ("stale_shift_detected_and_force_closed_with_flag",),
    "2.4.8": (
        "jammed_drawer_does_not_block_shift_close",
        "no_sale_open_is_logged_and_counted",
    ),
    "2.4.10": ("a_carried_float_is_declared_once_and_reconciles_across_both_shifts",),
    "2.5.1": ("x_report_does_not_reveal_expected_cash_to_the_closing_user",),
    "2.5.2": (
        "a_z_close_anchors_the_head",
        "z_report_counts_no_sale_opens",
        "z_belongs_to_the_shifts_business_date_not_the_wall_clock",
    ),
    "2.5.4": (
        "over_short_is_attributed_to_the_shift_and_its_opener_not_invented_per_cashier",
    ),
    "2.7.2": (
        "disabled_profile_produces_no_queue_row",
        "training_sale_produces_no_document",
        "zero_due_tender_completes_and_issues_a_fiscal_doc",
    ),
    "2.7.3": (
        "a_build_failure_becomes_build_failed_and_never_rejected",
        "a_rebuild_preserves_the_uuid_and_any_allocated_icv",
        "build_failed_is_excluded_from_dead_letter_count",
    ),
    "2.7.4": (
        "a_sale_completes_with_a_null_icv_and_allocates_on_reconnect",
        "a_store_scoped_counter_allocates_in_order_on_reconnect",
        "backoff_has_jitter",
        "prop_credit_note_never_precedes_its_invoice",
        "prop_icv_is_gap_free_and_strictly_increasing_within_its_scope",
    ),
    "2.7.7": (
        "an_ambiguous_timeout_resends_identical_bytes_under_the_same_uuid",
        "duplicate_recovery_follows_the_pinned_procedure",
        "rejection_dead_letters_verbatim_and_never_mutates_the_sale",
    ),
    "2.7.9": (
        "receipt_prints_without_qr_when_disabled",
        "reprint_is_byte_identical_including_qr",
    ),
    "2.7.12": (
        "production_build_refuses_mock_credentials",
        "tin_mismatch_in_response_alarms",
    ),
    "2.9.1": ("settlement_report_lists_unmatched_separately_by_direction",),
    "3.1.1": (
        "a_too_old_register_keeps_selling_and_says_so_in_device_health",
        "a_version_mismatch_never_dead_letters_a_fact",
        "an_unsupported_protocol_version_fails_the_batch_and_applies_nothing",
    ),
    "3.1.2": (
        "a_composite_foreign_key_refuses_a_cross_org_parent",
        "rls_is_forced_on_every_merchant_owned_table",
        "two_orgs_may_use_the_same_sku",
    ),
    "3.1.3": (
        "a_different_payload_under_a_known_uuid_is_rejected_and_alarms",
        "an_identical_replay_is_reported_as_duplicate",
        "an_incomplete_commit_group_is_held_not_partially_applied",
        "duplicate_batch_is_a_no_op",
        "partial_failure_acks_per_commit",
        "poison_commit_goes_to_dead_letter_without_blocking",
        "prop_apply_is_idempotent_under_any_replay_order",
        "the_stored_row_is_never_mutated_by_a_conflict",
    ),
    "3.1.6": (
        "http_routes_all_declare_a_capability",
        "prop_no_query_crosses_an_org_boundary",
    ),
    "3.1.7": ("two_offline_registers_never_allocate_the_same_icv",),
    "3.2.3": ("deep_outbox_alarms_without_blocking_sales",),
    "3.2.4": (
        "a_forked_checkpoint_is_refused_and_alarms",
        "tail_deletion_is_detected_against_the_server_checkpoint",
    ),
    "3.3.2": (
        "finalized_sales_are_never_touched",
        "open_cart_keeps_captured_price_after_catalog_apply",
        "reprice_cart_action_applies_new_prices",
    ),
    "3.3.5": (
        "a_fetched_bundle_is_never_written_to_the_local_database",
        "another_register_fetches_the_reprint_bundle_when_connected",
        "document_fetch_is_refused_offline_with_a_named_error",
    ),
    "3.4.7": ("email_bounce_logged_without_retry_storm",),
    "3.5.2": (
        "both_sales_of_the_last_unit_stand_and_stock_goes_negative_flagged",
        "offline_week_converges",
        "serial_refund_attempt_is_caught_when_connected_and_surfaced_when_not",
    ),
    "3.7.2": (
        "a_cloned_image_fails_its_first_authenticated_request",
        "device_id_collision_refuses_sync_with_a_named_error",
    ),
    "3.7.3": (
        "deactivation_applies_at_next_contact",
        "offline_auth_window_expires_and_says_why",
    ),
    "3.8.1": (
        "an_entitlement_for_another_org_is_rejected",
        "expiry_blocks_enrollment_and_updates",
        "grace_period_survives_a_long_outage",
        "licence_expiry_never_prevents_a_sale_on_an_entitled_register",
    ),
    "3.9.1": (
        "no_pii_in_a_captured_panic",
        "offline_telemetry_is_buffered_and_capped",
    ),
    "4.2.3": (
        "corrective_adjust_recomputes_wac",
        "ten_times_cost_requires_confirmation",
    ),
    "4.3.1": ("count_tolerates_sales_mid_count",),
    "4.3.3": ("short_receipt_creates_destination_adjustment_and_notifies",),
    "4.3.5": ("waste_adjustment_by_reason_code",),
    "4.4.6": ("single_use_code_marked_used_on_redemption_sync",),
    "4.6.3": (
        "catalog_save_above_ceiling_is_rejected",
        "sale_above_ceiling_is_hard_blocked",
    ),
    "4.7.4": ("override_report_groups_by_user_with_reasons",),
    "4.7.10": ("barcode_conflict_report_lists_both",),
    "4.8.3": ("second_store_refund_of_the_same_receipt_is_refused_when_connected",),
    "5.5.2": (
        "a_failed_update_before_migration_restores_the_previous_bundle",
        "a_post_migration_failure_restores_the_pre_update_snapshot_or_rolls_forward",
        "update_deferred_while_shift_open",
        "webview_cannot_invoke_the_updater_plugin",
    ),
    "5.5.3": ("all_migrations_run_against_soak_dataset_within_budget",),
}

PLANNED_CEILING = frozenset(
    """
a_backup_opens_with_the_recovery_code_alone
a_build_failure_becomes_build_failed_and_never_rejected
a_carried_float_is_declared_once_and_reconciles_across_both_shifts
a_cashier_has_a_path_forward_without_product_edit
a_checkout_operation_row_never_outlives_its_commit
a_cloned_image_fails_its_first_authenticated_request
a_composite_foreign_key_refuses_a_cross_org_parent
a_consumed_handle_is_still_consumed_after_restart
a_defective_refund_records_the_reason_code
a_department_sale_carries_its_own_tax_category_and_audits
a_different_actor_is_refused
a_different_payload_under_a_known_uuid_is_rejected_and_alarms
a_different_sale_is_refused
a_failed_update_before_migration_restores_the_previous_bundle
a_fetched_bundle_is_never_written_to_the_local_database
a_forked_checkpoint_is_refused_and_alarms
a_handle_used_twice_is_refused
a_multipack_barcode_adds_its_pack_quantity
a_pack_quantity_of_zero_is_refused_at_save
a_post_migration_failure_restores_the_pre_update_snapshot_or_rolls_forward
a_queued_artifact_prints_unchanged_once_a_printer_returns
a_rebuild_preserves_the_uuid_and_any_allocated_icv
a_safe_to_bank_movement_does_not_change_expected_drawer_cash
a_sale_completes_with_a_null_icv_and_allocates_on_reconnect
a_sale_completes_with_no_printer_and_queues_the_artifact
a_second_live_barcode_claim_is_refused
a_single_letter_query_does_not_match_a_vocalised_name
a_store_scoped_counter_allocates_in_order_on_reconnect
a_too_old_register_keeps_selling_and_says_so_in_device_health
a_version_mismatch_never_dead_letters_a_fact
a_z_close_anchors_the_head
age_decline_removes_line_and_audits
age_restricted_line_requires_confirmation
all_migrations_run_against_soak_dataset_within_budget
an_altered_amount_is_refused
an_ambiguous_timeout_resends_identical_bytes_under_the_same_uuid
an_entitlement_for_another_org_is_rejected
an_exchange_tender_is_never_cash_counted
an_expired_handle_is_refused
an_identical_replay_is_reported_as_duplicate
an_incomplete_commit_group_is_held_not_partially_applied
an_interrupted_tendering_is_recovered_and_status_queried
an_unknown_print_outcome_never_auto_retries_the_drawer_pulse
an_unsupported_protocol_version_fails_the_batch_and_applies_nothing
another_register_fetches_the_reprint_bundle_when_connected
backoff_has_jitter
bad_key_yields_recovery_state_not_panic
barcode_conflict_report_lists_both
both_sales_of_the_last_unit_stand_and_stock_goes_negative_flagged
build_failed_is_excluded_from_dead_letter_count
business_date_survives_timezone_change
card_charged_exact_unrounded_total
card_disabled_when_terminal_unreachable_cash_still_works
cash_refund_is_rounded_to_the_coin_step
catalog_save_above_ceiling_is_rejected
change_of_mind_outside_the_window_is_still_refused
clock_jump_back_reports_anomaly
corrective_adjust_recomputes_wac
count_tolerates_sales_mid_count
deactivation_applies_at_next_contact
deep_outbox_alarms_without_blocking_sales
defective_claim_bypasses_the_window_with_manager_approval
device_id_collision_refuses_sync_with_a_named_error
disabled_profile_produces_no_queue_row
displayed_price_override_queues_a_label_reprint
document_fetch_is_refused_offline_with_a_named_error
duplicate_batch_is_a_no_op
duplicate_recovery_follows_the_pinned_procedure
email_bounce_logged_without_retry_storm
exact_spelling_outranks_a_folded_variant
exchange_with_a_negative_difference_routes_to_the_original_card
every_movement_kind_has_a_term_in_expected_cash
expiry_blocks_enrollment_and_updates
finalize_is_atomic_under_injected_failure
finalized_sales_are_never_touched
fts_ignores_tatweel
fts_matches_alef_and_yaa_spelling_variants
fts_matches_arabic_with_and_without_diacritics
fts_matches_taa_marbuta_spelled_as_haa
fts_prefix_search_works_at_two_characters
golden_receipt_ar_58mm
golden_receipt_ar_80mm
grace_period_survives_a_long_outage
half_migrated_db_refuses_to_open_with_a_named_error
http_routes_all_declare_a_capability
inactive_product_cannot_be_added_but_can_be_refunded
interrupted_finalize_resumes_without_double_outbox_row
interrupted_finalize_resumes_without_double_stock_event
jammed_drawer_does_not_block_shift_close
key_generation_refuses_when_a_database_already_exists
licence_expiry_never_prevents_a_sale_on_an_entitled_register
low_disk_blocks_new_sales_and_alarms
manager_reset_retires_old_hash_and_audits
manager_self_approval_denied_when_policy_bans_it
mutating_an_identity_column_breaks_the_chain
narrow_profile_reflows_rather_than_truncates
no_command_argument_carries_a_price
no_pii_in_a_captured_panic
no_sale_open_is_logged_and_counted
offline_auth_window_expires_and_says_why
offline_telemetry_is_buffered_and_capped
offline_week_converges
open_cart_keeps_captured_price_after_catalog_apply
over_short_is_attributed_to_the_shift_and_its_opener_not_invented_per_cashier
override_below_floor_is_denied
override_report_groups_by_user_with_reasons
parked_carts_survive_restart
paid_in_from_safe_adjusts_expected_cash
partial_approval_leaves_remaining_due
partial_failure_acks_per_commit
partial_return_of_a_multibuy_reprices_the_remainder
partial_then_abandon_reverses
pending_tender_is_never_silently_dropped
pending_tender_polls_by_reference_before_declaring_unpaid
poison_commit_goes_to_dead_letter_without_blocking
price_embedded_line_total_equals_the_label
price_embedded_stock_event_carries_the_derived_weight_flagged_estimated
print_failure_after_finalize_leaves_sale_complete
production_build_refuses_mock_credentials
prop_apply_is_idempotent_under_any_replay_order
prop_cash_rounding_only_on_final_cash_tender
prop_corrupt_digit_never_parses_clean
prop_credit_note_never_precedes_its_invoice
prop_cumulative_refunds_never_exceed_sold_qty
prop_discount_never_makes_a_line_negative
prop_exchange_pair_nets_to_the_customer_facing_difference
prop_icv_is_gap_free_and_strictly_increasing_within_its_scope
prop_monotonic_clock_never_decreases
prop_no_input_sequence_yields_two_tenders_for_one_auth
prop_no_query_crosses_an_org_boundary
prop_no_query_string_produces_a_database_error
prop_park_resume_roundtrip_is_identity
prop_partial_refunds_sum_to_the_line_total
prop_price_cart_is_invariant_under_line_reordering
prop_refund_never_leaves_the_customer_better_off_than_not_buying
prop_refund_rounding_keeps_expected_cash_exact
prop_refund_uses_original_rate
prop_refunding_every_unit_returns_the_line_total_exactly
prop_zero_total_cart_is_valid
queue_never_stalls_on_unknown_code
receipt_prints_without_qr_when_disabled
receiptless_denied_when_disabled
receiptless_respects_threshold_and_requires_manager
refund_api_error_offers_store_credit_with_manager_approval
refund_of_an_exchanged_item_follows_the_chain
refund_uses_original_price_after_a_price_change
rejection_dead_letters_verbatim_and_never_mutates_the_sale
reprice_cart_action_applies_new_prices
reprint_is_byte_identical_including_qr
reversal_failure_escalates_and_audits
rls_is_forced_on_every_merchant_owned_table
sale_above_ceiling_is_hard_blocked
sale_at_0100_belongs_to_previous_business_date
sale_screen_min_size_guard
search_survives_every_fts5_metacharacter
second_store_refund_of_the_same_receipt_is_refused_when_connected
serial_refund_attempt_is_caught_when_connected_and_surfaced_when_not
settlement_report_lists_unmatched_separately_by_direction
short_receipt_creates_destination_adjustment_and_notifies
single_use_code_marked_used_on_redemption_sync
stale_shift_detected_and_force_closed_with_flag
status_query_approved_attaches_tender
stored_value_is_online_authorize_only_by_default
tail_deletion_is_detected_against_the_last_anchor
tail_deletion_is_detected_against_the_server_checkpoint
ten_times_cost_requires_confirmation
the_effect_and_the_consumption_commit_together_or_not_at_all
the_missing_printer_is_an_alarm_not_a_modal
the_stored_row_is_never_mutated_by_a_conflict
the_wrapped_envelope_travels_with_every_backup
tin_mismatch_in_response_alarms
training_auto_off_at_shift_close
training_sale_produces_no_document
training_sales_are_excluded_with_a_visible_count
two_offline_registers_never_allocate_the_same_icv
two_offline_registers_cannot_both_spend_the_same_balance
two_offline_registers_selling_the_last_unit_both_succeed
two_orgs_may_use_the_same_sku
unicode_names_roundtrip_through_db_and_fts
unknown_barcode_offers_quick_add_or_department_sale
unknown_triggers_status_query_before_any_retry
update_deferred_while_shift_open
user_switch_inside_an_open_shift_is_refused_when_the_policy_forbids_it
waste_adjustment_by_reason_code
webview_cannot_invoke_the_updater_plugin
x_report_does_not_reveal_expected_cash_to_the_closing_user
z_belongs_to_the_shifts_business_date_not_the_wall_clock
z_report_counts_no_sale_opens
zero_due_tender_completes_and_issues_a_fiscal_doc
""".split()
)

# A name moves here when it leaves PLANNED. Keeping the tombstone is what turns
# the initial ceiling into a monotonic history rather than a one-time maximum.
PLANNED_RETIRED: frozenset[str] = frozenset(
    {"manager_self_approval_denied_when_policy_bans_it"}
)

class CatalogFormatError(Exception):
    """The normative input could not be interpreted safely."""


class RunnerError(Exception):
    """A runner listing could not be produced or interpreted safely."""


@dataclass
class CatalogCase:
    case_id: str
    test_cell: str
    phase_cell: str
    phases: frozenset[int]
    kinds: frozenset[str]
    identifiers: tuple[str, ...]
    line: int
    continuation: list[str] = field(default_factory=list)

    @property
    def number(self) -> int | None:
        match = CASE_ID.fullmatch(self.case_id)
        if match is None or match.group("suffix"):
            return None
        return int(match.group("number"))

    @property
    def runner(self) -> str | None:
        groups: set[str] = set()
        if self.kinds & RUST_KINDS:
            groups.add("rust")
        if "web" in self.kinds:
            groups.add("web")
        if "fuzz" in self.kinds:
            groups.add("fuzz")
        if len(groups) > 1:
            raise CatalogFormatError(
                f"{CATALOG}:{self.line}: case {self.case_id} mixes runner kinds "
                f"in {', '.join(sorted(self.kinds))}"
            )
        return next(iter(groups), None)

    @property
    def status(self) -> str:
        text = " ".join((self.test_cell, *self.continuation))
        markers = [marker for marker in STATUS_MARKERS if marker in text]
        if len(markers) > 1:
            raise CatalogFormatError(
                f"{CATALOG}:{self.line}: case {self.case_id} has conflicting "
                f"status markers: {', '.join(markers)}"
            )
        return markers[0] if markers else TESTED


@dataclass(frozen=True)
class SummaryRow:
    marker: str
    count: int
    cases: tuple[int, ...] | None
    cases_cell: str
    line: int


@dataclass(frozen=True)
class CoverageSummary:
    rows: tuple[SummaryRow, ...]
    addends: tuple[int, ...]
    total: int
    equation_line: int


@dataclass(frozen=True)
class CatalogData:
    cases: tuple[CatalogCase, ...]
    summary: CoverageSummary


@dataclass(frozen=True)
class Citation:
    case_id: str
    phase: int
    path: Path
    line: int
    context: str


@dataclass(frozen=True)
class ReferenceTest:
    name: str
    path: Path
    line: int
    context: str


@dataclass(frozen=True)
class PhaseData:
    phase: int
    path: Path
    steps: Mapping[str, frozenset[str]]
    citations: tuple[Citation, ...]


@dataclass(frozen=True)
class RunnerInventory:
    rust: frozenset[str]
    web: frozenset[str]
    fuzz: frozenset[str]

    def names(self, runner: str) -> frozenset[str]:
        return getattr(self, runner)


@dataclass(frozen=True)
class Violation:
    assertion: int
    case_id: str
    identifier: str
    detail: str
    location: str = ""

    def render(self) -> str:
        suffix = f" ({self.location})" if self.location else ""
        return (
            f"assertion {self.assertion}: case {self.case_id}, identifier "
            f"`{self.identifier}` — {self.detail}{suffix}"
        )


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise CatalogFormatError(f"cannot read {path}: {error}") from error


def split_markdown_row(line: str, *, path: Path, number: int) -> list[str]:
    stripped = line.strip()
    if not (stripped.startswith("|") and stripped.endswith("|")):
        raise CatalogFormatError(f"{path}:{number}: malformed Markdown table row")

    cells: list[str] = []
    current: list[str] = []
    body = stripped[1:-1]
    code_fence = 0
    index = 0
    while index < len(body):
        char = body[index]
        if char == "`":
            end = index
            while end < len(body) and body[end] == "`":
                end += 1
            width = end - index
            if code_fence == 0:
                code_fence = width
            elif code_fence == width:
                code_fence = 0
            current.append(body[index:end])
            index = end
            continue
        if char == "|" and code_fence == 0 and (index == 0 or body[index - 1] != "\\"):
            cells.append("".join(current).strip())
            current = []
        else:
            current.append(char)
        index += 1
    cells.append("".join(current).strip())
    return cells


def is_table_separator(cells: Sequence[str], width: int) -> bool:
    return len(cells) == width and all(
        re.fullmatch(r":?-{3,}:?", cell) is not None for cell in cells
    )


def snake_code_spans(text: str) -> tuple[str, ...]:
    return tuple(
        match.group(1)
        for match in CODE_SPAN.finditer(text)
        if SNAKE_CASE.fullmatch(match.group(1))
    )


def cited_case_ids(text: str, *, path: Path, line: int) -> tuple[str, ...]:
    masked = list(text)
    case_ids: list[str] = []
    for match in E_RANGE.finditer(text):
        start = int(match.group("start"))
        end = int(match.group("end"))
        if start > end:
            raise CatalogFormatError(
                f"{path}:{line}: descending edge-case range `{match.group(0)}`"
            )
        case_ids.extend(str(case_id) for case_id in range(start, end + 1))
        masked[match.start() : match.end()] = " " * (match.end() - match.start())
    case_ids.extend(
        match.group("case") for match in E_REFERENCE.finditer("".join(masked))
    )
    return tuple(case_ids)


def parse_phases(cell: str, *, path: Path, line: int) -> frozenset[int]:
    value = cell.strip()
    if value == "—":
        return frozenset()
    match = re.fullmatch(
        r"(?P<phases>[1-5](?:\s*/\s*[1-5])*)(?:\s+→\s+owner needed)?",
        value,
    )
    if match is None:
        raise CatalogFormatError(f"{path}:{line}: cannot parse Ph cell `{cell}`")
    parsed = [int(phase) for phase in re.findall(r"[1-5]", match.group("phases"))]
    if len(parsed) != len(set(parsed)):
        raise CatalogFormatError(f"{path}:{line}: Ph cell repeats a phase: `{cell}`")
    return frozenset(parsed)


def parse_kinds(cell: str, *, path: Path, line: int) -> frozenset[str]:
    value = cell.strip()
    if value == "—":
        return frozenset()
    if re.fullmatch(r"[a-z]+(?:\s+\+\s+[a-z]+)*", value) is None:
        raise CatalogFormatError(f"{path}:{line}: cannot parse Kind cell `{cell}`")
    parsed = re.findall(r"[a-z]+", value)
    unknown = set(parsed) - KNOWN_KINDS
    if unknown:
        raise CatalogFormatError(
            f"{path}:{line}: unknown Kind value(s): {', '.join(sorted(unknown))}"
        )
    if len(parsed) != len(set(parsed)):
        raise CatalogFormatError(f"{path}:{line}: Kind cell repeats a kind: `{cell}`")
    return frozenset(parsed)


def parse_summary_cases(cell: str, *, path: Path, line: int) -> tuple[int, ...] | None:
    if cell.startswith("all except"):
        return None
    if cell == "—":
        return ()
    values: list[int] = []
    for part in cell.split(","):
        match = re.match(r"\s*([1-9][0-9]*)(?:\s|\*|$)", part)
        if match is None:
            raise CatalogFormatError(
                f"{path}:{line}: cannot parse coverage-summary Cases cell `{cell}`"
            )
        values.append(int(match.group(1)))
    return tuple(values)


def marker_in(text: str) -> str | None:
    matches = [marker for marker in STATUS_MARKERS if marker in text]
    if len(matches) != 1:
        return None
    return matches[0]


def parse_coverage_summary(
    lines: Sequence[str], start: int, path: Path
) -> CoverageSummary:
    header_index = -1
    for index in range(start + 1, len(lines)):
        if not lines[index].lstrip().startswith("|"):
            continue
        cells = split_markdown_row(lines[index], path=path, number=index + 1)
        if cells == ["Status", "Count", "Cases"]:
            header_index = index
            break
    if header_index < 0 or header_index + 1 >= len(lines):
        raise CatalogFormatError(f"{path}: Coverage summary table is missing")

    separator = split_markdown_row(
        lines[header_index + 1], path=path, number=header_index + 2
    )
    if not is_table_separator(separator, 3):
        raise CatalogFormatError(
            f"{path}:{header_index + 2}: malformed Coverage summary separator"
        )

    rows: list[SummaryRow] = []
    index = header_index + 2
    while index < len(lines) and lines[index].lstrip().startswith("|"):
        cells = split_markdown_row(lines[index], path=path, number=index + 1)
        if len(cells) != 3:
            raise CatalogFormatError(
                f"{path}:{index + 1}: Coverage summary row needs three columns"
            )
        marker = marker_in(cells[0])
        if marker is None:
            raise CatalogFormatError(
                f"{path}:{index + 1}: Coverage summary status needs one marker"
            )
        try:
            count = int(cells[1])
        except ValueError as error:
            raise CatalogFormatError(
                f"{path}:{index + 1}: Coverage summary Count is not an integer"
            ) from error
        rows.append(
            SummaryRow(
                marker=marker,
                count=count,
                cases=parse_summary_cases(cells[2], path=path, line=index + 1),
                cases_cell=cells[2],
                line=index + 1,
            )
        )
        index += 1

    equation = re.compile(
        r"^\s*(?P<terms>[0-9]+(?:\s*\+\s*[0-9]+)+)\s*=\s*(?P<total>[0-9]+)\."
    )
    for equation_index in range(index, len(lines)):
        match = equation.match(lines[equation_index])
        if match is None:
            continue
        addends = tuple(int(value.strip()) for value in match.group("terms").split("+"))
        return CoverageSummary(
            rows=tuple(rows),
            addends=addends,
            total=int(match.group("total")),
            equation_line=equation_index + 1,
        )
    raise CatalogFormatError(f"{path}: Coverage summary arithmetic is missing")


def parse_catalog(path: Path) -> CatalogData:
    lines = read_text(path).splitlines()
    try:
        edge_limit = lines.index("## The invariant properties")
        summary_start = lines.index("## Coverage summary")
    except ValueError as error:
        raise CatalogFormatError(
            f"{path}: required catalogue section is missing"
        ) from error

    cases: list[CatalogCase] = []
    table_header = ["#", "Case", "Test", "Ph", "Kind"]
    index = 0
    while index < edge_limit:
        line = lines[index]
        if not line.lstrip().startswith("|"):
            index += 1
            continue
        cells = split_markdown_row(line, path=path, number=index + 1)
        if cells != table_header:
            index += 1
            continue
        if index + 1 >= edge_limit:
            raise CatalogFormatError(
                f"{path}:{index + 1}: edge-case table is truncated"
            )
        separator = split_markdown_row(lines[index + 1], path=path, number=index + 2)
        if not is_table_separator(separator, 5):
            raise CatalogFormatError(
                f"{path}:{index + 2}: malformed edge-case table separator"
            )

        index += 2
        previous: CatalogCase | None = None
        while index < edge_limit and lines[index].lstrip().startswith("|"):
            cells = split_markdown_row(lines[index], path=path, number=index + 1)
            if len(cells) != 5:
                raise CatalogFormatError(
                    f"{path}:{index + 1}: edge-case row needs five columns"
                )
            raw_case = cells[0]
            if not raw_case:
                if previous is None:
                    raise CatalogFormatError(
                        f"{path}:{index + 1}: continuation row has no case"
                    )
                if cells[1] or cells[3] or cells[4]:
                    raise CatalogFormatError(
                        f"{path}:{index + 1}: continuation row must leave "
                        "Case, Ph and Kind empty"
                    )
                if not cells[2]:
                    raise CatalogFormatError(
                        f"{path}:{index + 1}: continuation Test cell is empty"
                    )
                previous.continuation.append(cells[2])
                previous.identifiers = (
                    *previous.identifiers,
                    *snake_code_spans(cells[2]),
                )
                index += 1
                continue
            if CASE_ID.fullmatch(raw_case) is None:
                raise CatalogFormatError(
                    f"{path}:{index + 1}: invalid case identifier `{raw_case}`"
                )
            previous = CatalogCase(
                case_id=raw_case,
                test_cell=cells[2],
                phase_cell=cells[3],
                phases=parse_phases(cells[3], path=path, line=index + 1),
                kinds=parse_kinds(cells[4], path=path, line=index + 1),
                identifiers=snake_code_spans(cells[2]),
                line=index + 1,
            )
            cases.append(previous)
            index += 1

    if not cases:
        raise CatalogFormatError(f"{path}: no edge-case rows were found")
    return CatalogData(
        cases=tuple(cases),
        summary=parse_coverage_summary(lines, summary_start, path),
    )


def update_fence(
    line: str, fence: tuple[str, int] | None
) -> tuple[tuple[str, int] | None, bool]:
    """Return the next fence and whether this Markdown line is fenced."""
    match = FENCE_LINE.match(line)
    if fence is None:
        if match is None:
            return None, False
        marker = match.group("marker")
        return (marker[0], len(marker)), True

    if match is not None:
        marker = match.group("marker")
        if (
            marker[0] == fence[0]
            and len(marker) >= fence[1]
            and not match.group("tail").strip()
        ):
            return None, True
    return fence, True


def parse_phase(path: Path, phase: int) -> PhaseData:
    lines = read_text(path).splitlines()
    mutable_steps: dict[str, set[str]] = {}
    current_step: str | None = None
    citations: list[Citation] = []
    in_exit_gate = False
    fence: tuple[str, int] | None = None

    for index, line in enumerate(lines, start=1):
        fence, is_fenced = update_fence(line, fence)
        if is_fenced:
            continue

        heading = MICROSTEP_HEADING.match(line)
        if line.startswith("## ") or (line.startswith("### ") and heading is None):
            current_step = None
        if heading is not None:
            current_step = heading.group("step")
            if not current_step.startswith(f"{phase}."):
                raise CatalogFormatError(
                    f"{path}:{index}: microstep `{current_step}` is in the wrong phase file"
                )
            if current_step in mutable_steps:
                raise CatalogFormatError(
                    f"{path}:{index}: duplicate microstep `{current_step}`"
                )
            mutable_steps[current_step] = set()

        if line == "## Exit gate":
            in_exit_gate = True
        elif in_exit_gate and line.startswith("## "):
            in_exit_gate = False

        is_tests_line = line.startswith("**Tests:**")
        if is_tests_line and not line.removeprefix("**Tests:**").strip():
            raise CatalogFormatError(
                f"{path}:{index}: Tests line has no evidence on the same line"
            )
        if is_tests_line and current_step is not None:
            mutable_steps[current_step].update(snake_code_spans(line))

        if is_tests_line or in_exit_gate:
            context = "Tests line" if is_tests_line else "exit gate"
            citations.extend(
                Citation(
                    case_id=case_id,
                    phase=phase,
                    path=path,
                    line=index,
                    context=context,
                )
                for case_id in cited_case_ids(line, path=path, line=index)
            )

    if fence is not None:
        raise CatalogFormatError(f"{path}: unclosed Markdown code fence")
    if not mutable_steps:
        raise CatalogFormatError(f"{path}: no microstep headings were found")
    return PhaseData(
        phase=phase,
        path=path,
        steps={step: frozenset(names) for step, names in mutable_steps.items()},
        citations=tuple(citations),
    )


REFERENCE_TEST_LINE = re.compile(
    r"^(?:\*\*(?:Tests|Properties):\*\*|"
    r"(?:Tests|Properties)(?:\s+—\s+\[[^\]]+\])?:)\s*(?P<body>.*)$"
)
REFERENCE_FIXTURE_LINE = re.compile(
    r"^(?:\*\*Fixture:\*\*|Fixture:)\s*(?P<body>.*)$"
)
REFERENCE_TEST_MENTION = re.compile(
    r"\b(?:Tests?|Properties)\s+(?P<body>`[a-z][a-z0-9_`., ·+()\-]*.*)$"
)
REFERENCE_TEST_COVER = re.compile(
    r"\bTests?[^`\n]{0,100}\bcover(?:s|ed)?\s+(?P<body>`.+)$"
)
REFERENCE_PHASE_EVIDENCE = re.compile(
    r"^Phase [1-5] (?:proves|owns)\s+(?P<body>`.+)$"
)
NORMATIVE_TEST_HEADERS = frozenset({"Test", "Property"})


def continued_reference_body(
    lines: Sequence[str], index: int, initial: str
) -> tuple[str, int]:
    """Join a normative evidence line with following backtick-led continuations."""
    body = [initial.strip()]
    cursor = index + 1
    while (
        cursor < len(lines)
        and lines[cursor].lstrip().startswith("`")
        and body[-1].rstrip().endswith(("·", ",", " and"))
    ):
        body.append(lines[cursor].strip())
        cursor += 1
    return "\n".join(piece for piece in body if piece), cursor


def normative_evidence_names(body: str) -> tuple[str, ...]:
    """Read identifiers that lead list items, not code spans in explanations."""
    names: list[str] = []
    for item in re.split(r"\s+·\s+|\n", body):
        candidate = item.strip()
        if not candidate.startswith("`"):
            continue
        candidate = candidate.split(" — ", 1)[0]
        names.extend(snake_code_spans(candidate))
    return tuple(names)


def parse_reference_tests(path: Path) -> tuple[ReferenceTest, ...]:
    """Read names that a reference document presents as normative evidence."""
    lines = read_text(path).splitlines()
    tests: list[ReferenceTest] = []
    fence: tuple[str, int] | None = None
    index = 0

    while index < len(lines):
        line_number = index + 1
        line = lines[index]
        fence, is_fenced = update_fence(line, fence)
        if is_fenced:
            index += 1
            continue

        next_index = index + 1
        inline = REFERENCE_TEST_LINE.match(line)
        fixture = REFERENCE_FIXTURE_LINE.match(line)
        if inline is not None or fixture is not None:
            evidence = inline if inline is not None else fixture
            assert evidence is not None
            body, next_index = continued_reference_body(
                lines, index, evidence.group("body")
            )
            names = normative_evidence_names(body)
            if not names:
                raise CatalogFormatError(
                    f"{path}:{line_number}: normative Tests/Properties/Fixture line has no "
                    "backtick-quoted snake_case identifier"
                )
            context = (
                "Tests/Properties line" if inline is not None else "Fixture line"
            )
            tests.extend(
                ReferenceTest(name, path, line_number, context)
                for name in names
            )
        elif (cover := REFERENCE_TEST_COVER.search(line)) is not None:
            tests.extend(
                ReferenceTest(name, path, line_number, "test coverage prose")
                for name in normative_evidence_names(cover.group("body"))
            )
        elif (phase_evidence := REFERENCE_PHASE_EVIDENCE.match(line)) is not None:
            tests.extend(
                ReferenceTest(name, path, line_number, "phase evidence prose")
                for name in normative_evidence_names(phase_evidence.group("body"))
            )
        elif (mention := REFERENCE_TEST_MENTION.search(line)) is not None:
            tests.extend(
                ReferenceTest(name, path, line_number, "test/property prose")
                for name in normative_evidence_names(mention.group("body"))
            )

        if not line.lstrip().startswith("|"):
            index = next_index
            continue
        header = split_markdown_row(line, path=path, number=line_number)
        if not header or header[0] not in NORMATIVE_TEST_HEADERS:
            index += 1
            continue
        if index + 1 >= len(lines):
            raise CatalogFormatError(f"{path}:{line_number}: normative test table is truncated")
        separator = split_markdown_row(
            lines[index + 1], path=path, number=line_number + 1
        )
        if not is_table_separator(separator, len(header)):
            raise CatalogFormatError(
                f"{path}:{line_number + 1}: malformed normative test-table separator"
            )

        index += 2
        while index < len(lines) and lines[index].lstrip().startswith("|"):
            row_number = index + 1
            cells = split_markdown_row(lines[index], path=path, number=row_number)
            if len(cells) != len(header):
                raise CatalogFormatError(
                    f"{path}:{row_number}: normative test row needs {len(header)} columns"
                )
            names = snake_code_spans(cells[0])
            if not names:
                raise CatalogFormatError(
                    f"{path}:{row_number}: normative {header[0]} cell has no "
                    "backtick-quoted snake_case identifier"
                )
            tests.extend(
                ReferenceTest(name, path, row_number, f"{header[0]} table")
                for name in names
            )
            index += 1

    if fence is not None:
        raise CatalogFormatError(f"{path}: unclosed Markdown code fence")
    return tuple(tests)


def load_reference_tests(root: Path) -> tuple[ReferenceTest, ...]:
    reference_dir = root / REFERENCE_DIR.relative_to(ROOT)
    tests: list[ReferenceTest] = []
    for path in sorted(reference_dir.glob("*.md")):
        tests.extend(parse_reference_tests(path))
    return tuple(tests)


def relative(path: Path, root: Path) -> str:
    try:
        return str(path.relative_to(root))
    except ValueError:
        return str(path)


def command_error(
    command: Sequence[str], result: subprocess.CompletedProcess[str]
) -> RunnerError:
    detail = result.stderr.strip() or result.stdout.strip() or "no diagnostic output"
    if len(detail) > 800:
        detail = f"{detail[:800]}…"
    return RunnerError(
        f"`{' '.join(command)}` exited {result.returncode}; runner listing unavailable: {detail}"
    )


def run_listing(command: Sequence[str], *, cwd: Path) -> str:
    try:
        result = subprocess.run(
            command,
            cwd=cwd,
            check=False,
            capture_output=True,
            text=True,
        )
    except (OSError, UnicodeError) as error:
        raise RunnerError(
            f"cannot run `{' '.join(command)}`; runner listing unavailable: {error}"
        ) from error
    if result.returncode != 0:
        raise command_error(command, result)
    return result.stdout


def list_rust_tests(root: Path) -> frozenset[str]:
    command = ("cargo", "nextest", "list", "--workspace", "--message-format", "json")
    output = run_listing(command, cwd=root)
    try:
        data = json.loads(output)
        suites = data["rust-suites"]
        if not isinstance(suites, dict):
            raise TypeError("rust-suites is not an object")
        names: set[str] = set()
        for suite in suites.values():
            testcases = suite["testcases"]
            if not isinstance(testcases, dict) or not all(
                isinstance(test_name, str) for test_name in testcases
            ):
                raise TypeError("nextest testcases is not a name-keyed object")
            names.update(test_name.rsplit("::", 1)[-1] for test_name in testcases)
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        raise RunnerError(f"nextest returned unparseable JSON: {error}") from error
    return frozenset(names)


def vitest_packages(root: Path) -> tuple[Path, ...]:
    packages: list[Path] = []
    for parent_name in ("apps", "packages"):
        parent = root / parent_name
        if not parent.is_dir():
            continue
        for manifest in sorted(parent.glob("*/package.json")):
            try:
                data = json.loads(read_text(manifest))
            except json.JSONDecodeError as error:
                raise RunnerError(f"cannot parse {manifest}: {error}") from error
            if not isinstance(data, dict):
                raise RunnerError(f"cannot parse {manifest}: root is not an object")
            dependencies = data.get("dependencies", {})
            dev_dependencies = data.get("devDependencies", {})
            scripts = data.get("scripts", {})
            if not all(
                isinstance(value, dict)
                for value in (dependencies, dev_dependencies, scripts)
            ):
                raise RunnerError(
                    f"cannot parse {manifest}: dependencies and scripts must be objects"
                )
            dependency_names = set(dependencies) | set(dev_dependencies)
            test_script = scripts.get("test", "")
            if not isinstance(test_script, str):
                raise RunnerError(
                    f"cannot parse {manifest}: test script is not a string"
                )
            if "vitest" in dependency_names or "vitest" in test_script:
                packages.append(manifest.parent)
    return tuple(packages)


def list_web_tests(root: Path) -> frozenset[str]:
    packages = vitest_packages(root)
    if not packages:
        raise RunnerError("catalogue has web tests but no Vitest package was found")
    names: set[str] = set()
    for package in packages:
        command = (
            "pnpm",
            "--dir",
            str(package),
            "exec",
            "vitest",
            "list",
            "--json",
        )
        output = run_listing(command, cwd=root)
        try:
            entries = json.loads(output)
            if not isinstance(entries, list):
                raise TypeError("Vitest listing is not an array")
            for entry in entries:
                full_name = entry["name"]
                if not isinstance(full_name, str):
                    raise TypeError("Vitest test name is not a string")
                names.add(full_name)
                names.add(full_name.rsplit(" > ", 1)[-1])
        except (json.JSONDecodeError, KeyError, TypeError) as error:
            raise RunnerError(
                f"Vitest returned unparseable JSON for {relative(package, root)}: {error}"
            ) from error
    return frozenset(names)


def list_fuzz_targets(root: Path) -> frozenset[str]:
    names: set[str] = set()
    for parent_name in ("apps", "crates", "packages"):
        parent = root / parent_name
        if not parent.is_dir():
            continue
        names.update(
            path.stem
            for path in parent.glob("*/fuzz/fuzz_targets/*.rs")
            if path.is_file()
        )
    return frozenset(names)


def runner_inventory(root: Path, catalog: CatalogData) -> RunnerInventory:
    runners = {case.runner for case in catalog.cases}
    return RunnerInventory(
        rust=list_rust_tests(root) if "rust" in runners else frozenset(),
        web=list_web_tests(root) if "web" in runners else frozenset(),
        fuzz=list_fuzz_targets(root) if "fuzz" in runners else frozenset(),
    )


def case_sort_key(case_id: str) -> tuple[int, str]:
    match = CASE_ID.fullmatch(case_id)
    if match is None:
        return (sys.maxsize, case_id)
    return (int(match.group("number")), match.group("suffix"))


def planned_index(
    planned: Mapping[str, Sequence[str]],
) -> tuple[dict[str, str], list[tuple[str, str, str]]]:
    index: dict[str, str] = {}
    errors: list[tuple[str, str, str]] = []
    for step, names in planned.items():
        if not names:
            errors.append(("—", step, "PLANNED owner has no test identifiers"))
        for name in names:
            if name in index:
                errors.append(
                    (
                        "—",
                        name,
                        f"PLANNED assigns the identifier to both {index[name]} and {step}",
                    )
                )
            else:
                index[name] = step
    return index, errors


def references_by_identifier(catalog: CatalogData) -> dict[str, list[CatalogCase]]:
    references: dict[str, list[CatalogCase]] = {}
    for case in catalog.cases:
        for identifier in case.identifiers:
            references.setdefault(identifier, []).append(case)
    return references


def case_label(cases: Sequence[CatalogCase]) -> str:
    return "/".join(sorted({case.case_id for case in cases}, key=case_sort_key)) or "—"


def check_runner_names(
    catalog: CatalogData,
    inventory: RunnerInventory,
    planned: Mapping[str, Sequence[str]],
    phase_filter: int | None,
    root: Path,
) -> list[Violation]:
    planned_names, _ = planned_index(planned)
    problems: list[Violation] = []
    for case in catalog.cases:
        if phase_filter is not None and phase_filter not in case.phases:
            continue
        if case.kinds == {"drill"} or case.status in RUNNER_EXEMPT_STATUSES:
            continue
        runner = case.runner
        if case.status == TESTED and not case.identifiers:
            problems.append(
                Violation(
                    assertion=1,
                    case_id=case.case_id,
                    identifier="<none>",
                    detail="tested non-drill row has no backtick snake_case evidence",
                    location=f"{relative(CATALOG, root)}:{case.line}",
                )
            )
            continue
        if runner is None:
            for identifier in case.identifiers:
                problems.append(
                    Violation(
                        assertion=1,
                        case_id=case.case_id,
                        identifier=identifier,
                        detail=(
                            "Kind `—` assigns no runner; this row's status is "
                            "not runner-exempt"
                        ),
                        location=f"{relative(CATALOG, root)}:{case.line}",
                    )
                )
            continue
        available = inventory.names(runner)
        for identifier in case.identifiers:
            if identifier in available or identifier in planned_names:
                continue
            problems.append(
                Violation(
                    assertion=1,
                    case_id=case.case_id,
                    identifier=identifier,
                    detail=f"absent from the {runner} runner listing and PLANNED",
                    location=f"{relative(CATALOG, root)}:{case.line}",
                )
            )
    return problems


def check_planned_owners(
    catalog: CatalogData,
    phases: Mapping[int, PhaseData],
    planned: Mapping[str, Sequence[str]],
    phase_filter: int | None,
) -> list[Violation]:
    references = references_by_identifier(catalog)
    planned_names, index_errors = planned_index(planned)
    problems = [
        Violation(2, case_id, identifier, detail)
        for case_id, identifier, detail in index_errors
    ]
    steps = {step: phase for phase in phases.values() for step in phase.steps}

    for identifier, step in planned_names.items():
        cases = references.get(identifier, [])
        if phase_filter is not None and not any(
            phase_filter in case.phases for case in cases
        ):
            continue
        label = case_label(cases)
        if not SNAKE_CASE.fullmatch(identifier):
            problems.append(
                Violation(2, label, identifier, "PLANNED identifier is not snake_case")
            )
        if MICROSTEP.fullmatch(step) is None:
            problems.append(
                Violation(2, label, identifier, f"PLANNED owner `{step}` is malformed")
            )
            continue
        owner = steps.get(step)
        if owner is None:
            problems.append(
                Violation(
                    2, label, identifier, f"PLANNED owner `{step}` does not exist"
                )
            )
            continue
        if not cases:
            problems.append(
                Violation(
                    2, "—", identifier, "PLANNED identifier is not in a Test column"
                )
            )
            continue
        if identifier not in owner.steps[step]:
            problems.append(
                Violation(
                    2,
                    label,
                    identifier,
                    f"microstep {step} exists but its Tests line does not name this test",
                    f"{relative(owner.path, ROOT)}",
                )
            )
        owner_phase = int(step.split(".", 1)[0])
        mismatched = [case.case_id for case in cases if owner_phase not in case.phases]
        if mismatched:
            problems.append(
                Violation(
                    2,
                    "/".join(mismatched),
                    identifier,
                    f"PLANNED owner {step} is Phase {owner_phase}, outside catalog Ph",
                )
            )
        owner_missing = [
            case.case_id
            for case in cases
            if case.status == "⏳" or "owner needed" in case.phase_cell
        ]
        if owner_missing:
            problems.append(
                Violation(
                    2,
                    "/".join(owner_missing),
                    identifier,
                    "catalog status or Ph still says no owner, but PLANNED "
                    f"assigns {step}",
                )
            )
    return problems


def check_planned_shrinks(
    catalog: CatalogData,
    inventory: RunnerInventory,
    planned: Mapping[str, Sequence[str]],
    ceiling: frozenset[str],
    retired: frozenset[str],
    phase_filter: int | None,
) -> list[Violation]:
    references = references_by_identifier(catalog)
    planned_names, _ = planned_index(planned)
    problems: list[Violation] = []
    active = set(planned_names)
    for identifier in sorted((active | retired) - ceiling):
        cases = references.get(identifier, [])
        problems.append(
            Violation(
                3,
                case_label(cases),
                identifier,
                "name exceeds the frozen PLANNED ceiling",
            )
        )

    for identifier in sorted(active & retired):
        problems.append(
            Violation(
                3,
                case_label(references.get(identifier, [])),
                identifier,
                "retired PLANNED entry was reactivated",
            )
        )

    for identifier in sorted(ceiling - active - retired):
        problems.append(
            Violation(
                3,
                case_label(references.get(identifier, [])),
                identifier,
                "PLANNED entry disappeared without a PLANNED_RETIRED tombstone",
            )
        )

    for identifier in sorted(planned_names):
        cases = references.get(identifier, [])
        if phase_filter is not None and not any(
            phase_filter in case.phases for case in cases
        ):
            continue
        if cases and all(
            case.runner is not None and identifier in inventory.names(case.runner)
            for case in cases
        ):
            problems.append(
                Violation(
                    3,
                    case_label(cases),
                    identifier,
                    "the runner lists this test; remove the stale PLANNED entry",
                )
            )
    return problems


def check_phase_citations(
    catalog: CatalogData,
    phases: Mapping[int, PhaseData],
    phase_filter: int | None,
    root: Path,
) -> list[Violation]:
    by_id: dict[str, list[CatalogCase]] = {}
    for case in catalog.cases:
        by_id.setdefault(case.case_id, []).append(case)

    problems: list[Violation] = []
    for phase, document in phases.items():
        if phase_filter is not None and phase != phase_filter:
            continue
        for citation in document.citations:
            rows = by_id.get(citation.case_id, [])
            location = f"{relative(citation.path, root)}:{citation.line}"
            if not rows:
                problems.append(
                    Violation(
                        4,
                        citation.case_id,
                        f"E.{citation.case_id}",
                        f"{citation.context} has no catalog row",
                        location,
                    )
                )
                continue
            if not any(phase in row.phases for row in rows):
                declared = ", ".join(f"`{row.phase_cell}`" for row in rows)
                problems.append(
                    Violation(
                        4,
                        citation.case_id,
                        f"E.{citation.case_id}",
                        f"Phase {phase} {citation.context} conflicts with catalog Ph {declared}",
                        location,
                    )
                )
    return problems


def check_case_numbers(catalog: CatalogData, root: Path) -> list[Violation]:
    occurrences: dict[str, list[CatalogCase]] = {}
    for case in catalog.cases:
        occurrences.setdefault(case.case_id, []).append(case)

    problems: list[Violation] = []
    for case_id, rows in occurrences.items():
        if len(rows) > 1:
            lines = ", ".join(str(row.line) for row in rows)
            problems.append(
                Violation(
                    5,
                    case_id,
                    f"E.{case_id}",
                    f"case identifier occurs {len(rows)} times",
                    f"{relative(CATALOG, root)}:{lines}",
                )
            )

    for case in catalog.cases:
        match = CASE_ID.fullmatch(case.case_id)
        if match is None or not match.group("suffix"):
            continue
        base = match.group("number")
        if base not in occurrences:
            problems.append(
                Violation(
                    5,
                    case.case_id,
                    f"E.{case.case_id}",
                    f"suffix variant has no base case E.{base}",
                    f"{relative(CATALOG, root)}:{case.line}",
                )
            )

    numbers = [case.number for case in catalog.cases if case.number is not None]
    if not numbers:
        raise CatalogFormatError(f"{CATALOG}: no integer case identifiers were found")
    present = set(numbers)
    for number in sorted(set(range(1, max(numbers) + 1)) - present):
        problems.append(
            Violation(5, str(number), f"E.{number}", "numbered case is missing")
        )
    return problems


def check_coverage_summary(catalog: CatalogData, root: Path) -> list[Violation]:
    canonical: dict[int, CatalogCase] = {}
    for case in catalog.cases:
        if case.number is not None:
            canonical.setdefault(case.number, case)

    actual: dict[str, set[int]] = {marker: set() for marker in STATUS_MARKERS}
    for number, case in canonical.items():
        actual[case.status].add(number)

    summary_by_marker: dict[str, list[SummaryRow]] = {}
    for row in catalog.summary.rows:
        summary_by_marker.setdefault(row.marker, []).append(row)

    problems: list[Violation] = []
    for marker in STATUS_MARKERS:
        rows = summary_by_marker.get(marker, [])
        if len(rows) != 1:
            problems.append(
                Violation(
                    6,
                    "—",
                    marker,
                    f"Coverage summary has {len(rows)} rows for this status, expected one",
                )
            )
            continue
        row = rows[0]
        actual_cases = actual[marker]
        location = f"{relative(CATALOG, root)}:{row.line}"
        if row.count != len(actual_cases):
            problems.append(
                Violation(
                    6,
                    "—",
                    marker,
                    f"typed Count {row.count} disagrees with recomputed {len(actual_cases)}",
                    location,
                )
            )
        if marker == TESTED:
            if row.cases_cell != "all except those below":
                problems.append(
                    Violation(
                        6,
                        "—",
                        marker,
                        "tested Cases cell must be `all except those below`",
                        location,
                    )
                )
        elif row.cases is not None and len(row.cases) != len(set(row.cases)):
            problems.append(
                Violation(
                    6,
                    "—",
                    marker,
                    f"typed Cases {row.cases_cell!r} contain a duplicate case",
                    location,
                )
            )
        elif set(row.cases or ()) != actual_cases:
            problems.append(
                Violation(
                    6,
                    "—",
                    marker,
                    f"typed Cases {row.cases_cell!r} disagree with "
                    f"recomputed {', '.join(map(str, sorted(actual_cases))) or 'none'}",
                    location,
                )
            )

    typed_counts = tuple(row.count for row in catalog.summary.rows)
    expected_total = len(canonical)
    equation_location = f"{relative(CATALOG, root)}:{catalog.summary.equation_line}"
    if catalog.summary.addends != typed_counts:
        problems.append(
            Violation(
                6,
                "—",
                "coverage arithmetic",
                f"equation addends {catalog.summary.addends} do not match table counts "
                f"{typed_counts}",
                equation_location,
            )
        )
    if sum(catalog.summary.addends) != catalog.summary.total:
        problems.append(
            Violation(
                6,
                "—",
                "coverage arithmetic",
                "equation's left and right sides disagree",
                equation_location,
            )
        )
    if catalog.summary.total != expected_total:
        problems.append(
            Violation(
                6,
                "—",
                "coverage total",
                f"typed total {catalog.summary.total} disagrees with {expected_total} integer cases",
                equation_location,
            )
        )
    return problems


def check_reference_contracts(
    references: Sequence[ReferenceTest],
    phases: Mapping[int, PhaseData],
    phase_filter: int | None,
    root: Path,
) -> list[Violation]:
    """Require every normative reference name to have evidence and one owner."""
    owners: dict[str, list[tuple[int, str, Path]]] = {}
    for phase, document in phases.items():
        for step, names in document.steps.items():
            for name in names:
                owners.setdefault(name, []).append((phase, step, document.path))

    by_name: dict[str, list[ReferenceTest]] = {}
    for reference in references:
        by_name.setdefault(reference.name, []).append(reference)

    problems: list[Violation] = []
    for name, occurrences in sorted(by_name.items()):
        named_owners = owners.get(name, [])
        if phase_filter is not None and named_owners and all(
            phase != phase_filter for phase, _, _ in named_owners
        ):
            continue
        locations = ", ".join(
            f"{relative(item.path, root)}:{item.line}" for item in occurrences
        )
        if not named_owners:
            problems.append(
                Violation(
                    7,
                    "—",
                    name,
                    "normative reference name has no owning phase-microstep Tests line",
                    locations,
                )
            )
        elif len(named_owners) > 1:
            owner_list = ", ".join(
                f"{step} ({relative(path, root)})"
                for _, step, path in named_owners
            )
            problems.append(
                Violation(
                    7,
                    "—",
                    name,
                    f"normative reference name has multiple owners: {owner_list}",
                    locations,
                )
            )
    return problems


def evaluate(
    catalog: CatalogData,
    phases: Mapping[int, PhaseData],
    inventory: RunnerInventory,
    planned: Mapping[str, Sequence[str]],
    ceiling: frozenset[str],
    retired: frozenset[str],
    phase_filter: int | None,
    root: Path,
    reference_tests: Sequence[ReferenceTest] = (),
) -> list[Violation]:
    problems = [
        *check_runner_names(catalog, inventory, planned, phase_filter, root),
        *check_planned_owners(catalog, phases, planned, phase_filter),
        *check_planned_shrinks(
            catalog,
            inventory,
            planned,
            ceiling,
            retired,
            phase_filter,
        ),
        *check_phase_citations(catalog, phases, phase_filter, root),
        *check_case_numbers(catalog, root),
        *check_coverage_summary(catalog, root),
        *check_reference_contracts(
            reference_tests,
            phases,
            phase_filter,
            root,
        ),
    ]
    return sorted(
        problems,
        key=lambda item: (
            item.assertion,
            case_sort_key(item.case_id.split("/", 1)[0]),
            item.identifier,
            item.detail,
        ),
    )


def load_inputs(root: Path) -> tuple[CatalogData, dict[int, PhaseData]]:
    catalog_path = root / "docs/implementation/ref/test-catalog.md"
    phase_paths = {
        phase: root / path.relative_to(ROOT) for phase, path in PHASE_FILES.items()
    }
    return parse_catalog(catalog_path), {
        phase: parse_phase(path, phase) for phase, path in phase_paths.items()
    }


def fixture_catalog(
    *,
    case_one: str = "rust_case_one",
    duplicate: bool = False,
    suffix_without_base: bool = False,
    tested_count: int = 2,
) -> str:
    duplicate_row = (
        "| 3 | Duplicate | `accepted_case_three` | 1 | integration |\n"
        if duplicate
        else ""
    )
    suffix_row = (
        "| 7b | Orphan suffix | **manual drill** | 1 | drill |\n"
        if suffix_without_base
        else ""
    )
    return f"""# Test catalog — fixture

## Matrix

| # | Case | Test | Ph | Kind |
|---|---|---|---|---|
| 1 | Runner evidence | `{case_one}` | 1 | unit |
| 2 | Planned evidence | `planned_case_two` | 1 | unit |
| 3 | Accepted risk | `accepted_case_three` | 1 | integration |
| | | ⚠️ **Accepted risk.** | | |
{duplicate_row}| 4 | Open answer | `open_case_four` | 1 | unit |
| | | ❓ **Open.** | | |
| 5 | Deferred | 🧩 **Deferred.** | — | — |
| 6 | Out | 🚫 **Out of scope.** | — | — |
{suffix_row}

## The invariant properties

## Coverage summary

| Status | Count | Cases |
|---|---|---|
| ✅ Tested | {tested_count} | all except those below |
| ⏳ Planned | 0 | — |
| ⚠️ Accepted | 1 | 3 |
| ❓ Open | 1 | 4 |
| 🧩 Deferred | 1 | 5 |
| 🚫 Out | 1 | 6 |

{tested_count} + 0 + 1 + 1 + 1 + 1 = 6.
"""


def write_fixture(
    root: Path,
    catalog_text: str,
    phase_two_citation: bool = False,
    phase_one_tests: Sequence[str] = ("planned_case_two",),
) -> None:
    catalog_path = root / "docs/implementation/ref/test-catalog.md"
    catalog_path.parent.mkdir(parents=True)
    catalog_path.write_text(catalog_text, encoding="utf-8")
    for phase, source in PHASE_FILES.items():
        path = root / source.relative_to(ROOT)
        path.parent.mkdir(parents=True, exist_ok=True)
        citation = " (E.1)" if phase == 2 and phase_two_citation else ""
        tests = phase_one_tests if phase == 1 else (f"fixture_phase_{phase}_test",)
        test_spans = " · ".join(f"`{name}`" for name in tests)
        path.write_text(
            f"# Phase {phase}\n\n### {phase}.1.1 — Fixture\n"
            f"**Tests:** {test_spans}{citation}\n\n## Exit gate\n",
            encoding="utf-8",
        )


def self_test() -> int:
    base_planned = {"1.1.1": ("planned_case_two",)}
    base_inventory = RunnerInventory(
        rust=frozenset({"rust_case_one", "accepted_case_three", "open_case_four"}),
        web=frozenset(),
        fuzz=frozenset(),
    )
    fixtures: list[
        tuple[
            str,
            str,
            Mapping[str, Sequence[str]],
            frozenset[str],
            frozenset[str],
            RunnerInventory,
            bool,
            int,
        ]
    ] = [
        (
            "assertion 1 refuses a missing runner name",
            fixture_catalog(),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            RunnerInventory(
                rust=frozenset({"accepted_case_three", "open_case_four"}),
                web=frozenset(),
                fuzz=frozenset(),
            ),
            False,
            1,
        ),
        (
            "assertion 1 refuses a tested row with no identifier",
            fixture_catalog().replace("`rust_case_one`", "**no test named**", 1),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            base_inventory,
            False,
            1,
        ),
        (
            "assertion 1 checks identifiers in continuation rows",
            fixture_catalog().replace(
                "| 1 | Runner evidence | `rust_case_one` | 1 | unit |",
                "| 1 | Runner evidence | `rust_case_one` | 1 | unit |\n"
                "| | | `continued_case_one` | | |",
                1,
            ),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            base_inventory,
            False,
            1,
        ),
        (
            "assertion 1 refuses a tested row with no runner",
            fixture_catalog().replace(
                "`rust_case_one` | 1 | unit",
                "`rust_case_one` | 1 | —",
                1,
            ),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            base_inventory,
            False,
            1,
        ),
        (
            "assertion 2 refuses a nonexistent PLANNED owner",
            fixture_catalog(),
            {"1.9.9": ("planned_case_two",)},
            frozenset({"planned_case_two"}),
            frozenset(),
            base_inventory,
            False,
            2,
        ),
        (
            "assertion 2 refuses a no-owner status after an owner is assigned",
            fixture_catalog()
            .replace(
                "| 2 | Planned evidence | `planned_case_two`",
                "| 2 | Planned evidence | ⏳ `planned_case_two`",
                1,
            )
            .replace("| ✅ Tested | 2 |", "| ✅ Tested | 1 |", 1)
            .replace("| ⏳ Planned | 0 | — |", "| ⏳ Planned | 1 | 2 |", 1)
            .replace("2 + 0 + 1 + 1 + 1 + 1", "1 + 1 + 1 + 1 + 1 + 1", 1),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            base_inventory,
            False,
            2,
        ),
        (
            "assertion 3 refuses PLANNED growth",
            fixture_catalog(case_one="new_planned_test"),
            {
                "1.1.1": ("planned_case_two", "new_planned_test"),
            },
            frozenset({"planned_case_two"}),
            frozenset(),
            RunnerInventory(
                rust=frozenset({"accepted_case_three", "open_case_four"}),
                web=frozenset(),
                fuzz=frozenset(),
            ),
            False,
            3,
        ),
        (
            "assertion 4 refuses a phase citation outside catalog Ph",
            fixture_catalog(),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            base_inventory,
            True,
            4,
        ),
        (
            "assertion 3 refuses reactivating a retired name",
            fixture_catalog(),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset({"planned_case_two"}),
            base_inventory,
            False,
            3,
        ),
        (
            "assertion 5 refuses duplicate case numbers",
            fixture_catalog(duplicate=True),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            base_inventory,
            False,
            5,
        ),
        (
            "assertion 5 refuses a suffix without its base case",
            fixture_catalog(suffix_without_base=True),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            base_inventory,
            False,
            5,
        ),
        (
            "assertion 6 recomputes summary counts",
            fixture_catalog(tested_count=3),
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            base_inventory,
            False,
            6,
        ),
    ]

    passed = failed = 0
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(root, fixture_catalog())
        catalog, phases = load_inputs(root)
        baseline_problems = evaluate(
            catalog,
            phases,
            base_inventory,
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            None,
            root,
        )
    if baseline_problems:
        print(
            "  FAIL  clean fixture passes "
            f"(got: {'; '.join(problem.render() for problem in baseline_problems)})"
        )
        failed += 1
    else:
        print("  ok    clean fixture passes")
        passed += 1

    exempt_catalog = (
        fixture_catalog()
        .replace(
            "⚠️ **Accepted risk.**",
            "⚠️ **Accepted risk.** `accepted_risk_hook`",
            1,
        )
        .replace(
            "🧩 **Deferred.**",
            "🧩 **Deferred.** `deferred_hook`",
            1,
        )
        .replace(
            "🚫 **Out of scope.**",
            "🚫 **Out of scope.** `out_of_scope_hook`",
            1,
        )
    )
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(root, exempt_catalog)
        catalog, phases = load_inputs(root)
        exempt_problems = evaluate(
            catalog,
            phases,
            base_inventory,
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            None,
            root,
        )
    exempt_names = {
        "accepted_risk_hook",
        "deferred_hook",
        "out_of_scope_hook",
    }
    if not any(
        problem.assertion == 1 and problem.identifier in exempt_names
        for problem in exempt_problems
    ):
        print("  ok    assertion 1 exempts accepted, deferred and out-of-scope hooks")
        passed += 1
    else:
        print(
            "  FAIL  assertion 1 exempts accepted, deferred and out-of-scope hooks "
            f"(got: {'; '.join(problem.render() for problem in exempt_problems)})"
        )
        failed += 1

    for (
        label,
        catalog_text,
        planned,
        ceiling,
        retired,
        inventory,
        citation,
        assertion,
    ) in fixtures:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_fixture(
                root,
                catalog_text,
                citation,
                tuple(planned.get("1.1.1", ("planned_case_two",))),
            )
            try:
                catalog, phases = load_inputs(root)
                problems = evaluate(
                    catalog,
                    phases,
                    inventory,
                    planned,
                    ceiling,
                    retired,
                    None,
                    root,
                )
            except (CatalogFormatError, RunnerError) as error:
                problems = []
                diagnostic = str(error)
            else:
                diagnostic = "; ".join(problem.render() for problem in problems)
        matched = any(problem.assertion == assertion for problem in problems)
        if matched:
            print(f"  ok    {label}")
            passed += 1
        else:
            print(f"  FAIL  {label} (got: {diagnostic or 'no violation'})")
            failed += 1

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(root, fixture_catalog())
        phase_path = root / PHASE_FILES[1].relative_to(ROOT)
        phase_text = read_text(phase_path).replace(
            "# Phase 1\n\n",
            "# Phase 1\n\n```markdown\n### 1.9.9 — Fake\n"
            "**Tests:** `planned_case_two`\n```\n\n",
            1,
        )
        phase_path.write_text(phase_text, encoding="utf-8")
        parsed_phase = parse_phase(phase_path, 1)
    if "1.9.9" not in parsed_phase.steps and "1.1.1" in parsed_phase.steps:
        print("  ok    fenced examples cannot create PLANNED owners")
        passed += 1
    else:
        print("  FAIL  fenced examples cannot create PLANNED owners")
        failed += 1

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(
            root,
            fixture_catalog(),
            phase_one_tests=("unrelated_test",),
        )
        phase_path = root / PHASE_FILES[1].relative_to(ROOT)
        phase_path.write_text(
            f"{read_text(phase_path)}**Tests:** `planned_case_two`\n",
            encoding="utf-8",
        )
        catalog, phases = load_inputs(root)
        exit_gate_problems = evaluate(
            catalog,
            phases,
            base_inventory,
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            None,
            root,
        )
    if any(problem.assertion == 2 for problem in exit_gate_problems):
        print("  ok    an exit-gate Tests line cannot forge a PLANNED owner")
        passed += 1
    else:
        print("  FAIL  an exit-gate Tests line cannot forge a PLANNED owner")
        failed += 1

    invalid_separator = fixture_catalog().replace("|---|---|---|---|---|", "||||||", 1)
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(root, invalid_separator)
        try:
            load_inputs(root)
        except CatalogFormatError:
            separator_was_refused = True
        else:
            separator_was_refused = False
    if separator_was_refused:
        print("  ok    malformed table separators are could-not-run errors")
        passed += 1
    else:
        print("  FAIL  malformed table separators are could-not-run errors")
        failed += 1

    invalid_phase = fixture_catalog().replace(
        "`rust_case_one` | 1 | unit", "`rust_case_one` | 1 / 9 | unit", 1
    )
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(root, invalid_phase)
        try:
            load_inputs(root)
        except CatalogFormatError:
            phase_was_refused = True
        else:
            phase_was_refused = False
    if phase_was_refused:
        print("  ok    malformed Ph cells are could-not-run errors")
        passed += 1
    else:
        print("  FAIL  malformed Ph cells are could-not-run errors")
        failed += 1

    reference_form_fixtures = (
        (
            "annotated Tests evidence is parsed",
            "Tests — [1.1.1]: `annotated_reference_case`.\n",
            {"annotated_reference_case"},
        ),
        (
            "multiline Tests evidence is parsed",
            "**Tests:** `first_multiline_case` ·\n"
            "`second_multiline_case`.\n",
            {"first_multiline_case", "second_multiline_case"},
        ),
        (
            "Tests-cover prose is parsed",
            "Tests and byte-stable cases cover `covered_reference_case`.\n",
            {"covered_reference_case"},
        ),
        (
            "Fixture evidence is parsed",
            "**Fixture:** `fixture_reference_case` — reconnect twice.\n",
            {"fixture_reference_case"},
        ),
        (
            "Phase ownership prose is parsed",
            "Phase 2 proves `proved_reference_case`.\n"
            "Phase 3 owns `owned_reference_case`.\n",
            {"proved_reference_case", "owned_reference_case"},
        ),
    )
    for label, source, expected_names in reference_form_fixtures:
        with tempfile.TemporaryDirectory() as temporary:
            reference_path = Path(temporary) / "normative.md"
            reference_path.write_text(source, encoding="utf-8")
            actual_names = {
                reference.name for reference in parse_reference_tests(reference_path)
            }
        if actual_names == expected_names:
            print(f"  ok    {label}")
            passed += 1
        else:
            print(
                f"  FAIL  {label} "
                f"(expected {sorted(expected_names)}, got {sorted(actual_names)})"
            )
            failed += 1

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(root, fixture_catalog())
        reference_path = root / "docs/implementation/ref/normative.md"
        reference_path.write_text(
            "| Test | Rule |\n|---|---|\n"
            "| `planned_case_two` | promised evidence |\n",
            encoding="utf-8",
        )
        catalog, phases = load_inputs(root)
        reference_problems = evaluate(
            catalog,
            phases,
            base_inventory,
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            None,
            root,
            load_reference_tests(root),
        )
    if not any(problem.assertion == 7 for problem in reference_problems):
        print("  ok    assertion 7 accepts a normative name with evidence and one owner")
        passed += 1
    else:
        print(
            "  FAIL  assertion 7 accepts a normative name with evidence and one owner "
            f"(got: {'; '.join(problem.render() for problem in reference_problems)})"
        )
        failed += 1

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(root, fixture_catalog())
        reference_path = root / "docs/implementation/ref/normative.md"
        reference_path.write_text(
            "**Tests:** `orphaned_reference_alias`.\n",
            encoding="utf-8",
        )
        catalog, phases = load_inputs(root)
        orphan_problems = evaluate(
            catalog,
            phases,
            base_inventory,
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            None,
            root,
            load_reference_tests(root),
        )
    orphan_failures = [
        problem
        for problem in orphan_problems
        if problem.assertion == 7
        and problem.identifier == "orphaned_reference_alias"
    ]
    if len(orphan_failures) == 1:
        print("  ok    assertion 7 refuses an orphaned normative reference alias")
        passed += 1
    else:
        print(
            "  FAIL  assertion 7 refuses an orphaned normative reference alias "
            f"(got: {'; '.join(problem.render() for problem in orphan_problems)})"
        )
        failed += 1

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_fixture(
            root,
            fixture_catalog(),
            phase_one_tests=("competing_owner_alias",),
        )
        reference_path = root / "docs/implementation/ref/normative.md"
        reference_path.write_text(
            "Tests `planned_case_two` pin the contract.\n",
            encoding="utf-8",
        )
        catalog, phases = load_inputs(root)
        alias_problems = evaluate(
            catalog,
            phases,
            base_inventory,
            base_planned,
            frozenset({"planned_case_two"}),
            frozenset(),
            None,
            root,
            load_reference_tests(root),
        )
    if any(
        problem.assertion == 7
        and problem.identifier == "planned_case_two"
        and "no owning" in problem.detail
        for problem in alias_problems
    ):
        print("  ok    assertion 7 exposes a competing phase-owner alias")
        passed += 1
    else:
        print(
            "  FAIL  assertion 7 exposes a competing phase-owner alias "
            f"(got: {'; '.join(problem.render() for problem in alias_problems)})"
        )
        failed += 1

    print(f"\ncheck-test-catalog self-test: {passed} passed, {failed} failed")
    return 1 if failed else 0


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="reconcile the edge-case coverage matrix"
    )
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "--self-test", action="store_true", help="prove all seven assertions fire"
    )
    group.add_argument(
        "--phase", type=int, choices=range(1, 6), help="limit evidence checks"
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.self_test:
        return self_test()

    try:
        catalog, phases = load_inputs(ROOT)
        reference_tests = load_reference_tests(ROOT)
        inventory = runner_inventory(ROOT, catalog)
        problems = evaluate(
            catalog,
            phases,
            inventory,
            PLANNED,
            PLANNED_CEILING,
            PLANNED_RETIRED,
            args.phase,
            ROOT,
            reference_tests,
        )
    except (CatalogFormatError, RunnerError) as error:
        print(f"test-catalog: ERROR — {error}", file=sys.stderr)
        return 2

    if problems:
        scope = f" for Phase {args.phase}" if args.phase is not None else ""
        print(f"{len(problems)} test-catalog violation(s){scope}:")
        for problem in problems:
            print(f"  FAIL  {problem.render()}")
        return 1

    integer_cases = sum(case.number is not None for case in catalog.cases)
    scope = f" for Phase {args.phase}" if args.phase is not None else ""
    print(
        f"test catalog reconciles runner listings, PLANNED owners, phase citations, "
        f"normative references, numbering and summary{scope} ({integer_cases} cases)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
