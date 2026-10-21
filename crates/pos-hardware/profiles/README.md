# Device profiles

No device is bought before milestone 5.8, so the devices this product is built against are data
([`ref/hardware-and-receipts.md`](../../../docs/implementation/ref/hardware-and-receipts.md) §6b).
Each file here is one **version** of one device profile: a receipt printer, a cash drawer, a barcode
scanner or a register's screen. The Rust emulators and suites read them through
`pos_hardware::profile::data`; the register's web tests read the same JSON directly. Microstep 1.7.11
seeded the first seventeen from the candidates
[`docs/hardware-lab.md`](../../../docs/hardware-lab.md) researched; every later class starts in the
microstep that builds its trait.

```
profiles/
  receipt_printer/<profile_id>.v<version>.json
  cash_drawer/…
  scanner/…
  register_screen/…
  versions.blake3          one committed hash per version
```

## One value

```json
"dot_width": { "value": 576, "source": "docs/hardware-lab.md §4.2, citing Epson, TM-T20III Technical Reference Guide, https://…" }
```

Every value is an object with exactly two keys. `value` is the fact, or the string `"unknown"` when
nobody has stated it — so `"unknown"` is reserved and no real value may be that string. `source`
says where the value came from, and for an unknown, where it was looked for.

## Three rules, each with a test

`crates/pos-hardware/tests/profiles.rs` holds them, and `cargo nextest run -p pos-hardware --test
profiles` runs them.

1. **Every value names its source** — a maker's document, a seller's written answer, or a capture
   from a device. A source cites a section of a document in this repository or an address outside
   it; an adjective does not load past `every_device_profile_names_a_source_for_every_value`. A
   profile that describes a class rather than a model (`"describes": "class"`) is synthetic, and every
   one of its sources begins by saying so.
2. **`unknown` is a value, and it is never filled by a guess.** Each class's values are a closed set:
   a file missing one does not load, and neither does a file naming one its class lacks. A receipt
   printer with an unknown in a parameter its driver needs cannot be bound to a register
   (`an_unknown_profile_value_cannot_be_bound_to_a_register`) — `pos_hardware::profile::BINDING`
   lists those parameters.
3. **A profile is versioned, never edited in place.** A changed value — a datasheet correction, a
   firmware note, the gate's evidence — is a new file beside the old one, because a capture, a
   fixture or a qualification may name the old one. `versions.blake3` commits a hash per version, so
   an edit in place fails `a_changed_profile_value_is_a_new_profile_version`.

## Adding a version

1. Copy the newest version of the profile to `<profile_id>.v<N+1>.json`, change `version` to match,
   and change the values with their new sources. Leave the old file exactly as it is.
2. Run `cargo nextest run -p pos-hardware --test profiles`. The version test fails, naming the new
   file and printing the manifest it expects; add the new file's line to `versions.blake3`, keeping
   the file sorted by path.
3. The first versions stay as they were seeded:
   `every_seeded_profile_is_unknown_exactly_where_the_seed_table_says` holds every `v1` to
   [`ref/hardware-and-receipts.md`](../../../docs/implementation/ref/hardware-and-receipts.md) §6b.2's
   seed table, and a later version is held only to the three rules.

A profile describes a candidate and claims nothing more: it is not a row of §6a's supported-device
matrix, which needs a qualifying commit against a device somebody held.
