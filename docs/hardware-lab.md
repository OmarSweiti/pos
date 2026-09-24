# The hardware lab — what to buy, how to set it up, and what it unblocks

**Tracks #68.** Written 24 September 2026 against `development` at `1ccdd63`. This is an operator
runbook, in the same family as [`phase-0-remaining-setup.md`](phase-0-remaining-setup.md): it tells a
person what to buy and do, and it hands the agent what it needs to continue. The plan of record is
still [`implementation/`](implementation/). Every requirement below cites the section it comes from,
and where this file and the plan disagree, **the plan is right**.

> **Candidates, not the matrix.** Models named here are examples that meet the plan's requirements
> on their makers' published specifications. **Nothing is supported until it has been bought, held
> and qualified**
> ([`ref/hardware-and-receipts.md`](implementation/ref/hardware-and-receipts.md) §6a: *"Filling it
> with model numbers nobody has held would be the same defect as a compliance claim nobody
> earned."*). Buying a device is step 1 of qualifying it, not the end of it.

Owners are marked 👤 **you** and 🤖 **agent** throughout.

---

## 0 · The answer in one screen

1. **Buy now. By the plan's own schedule, the order is already late.** The plan says *"Order the
   hardware before group 1.7 starts"* (`ref/hardware-and-receipts.md` §6a.1, and the master plan's
   long-lead register, §6a). Group 1.7 started on 20 September, and four of its ten steps have landed
   with no printer in the building.
2. **Buy a Windows x86-64 all-in-one touchscreen register, not an Android terminal.** The blueprint's
   own decision rule settles #68's open question (§3).
3. **Buy the *lowest* machine you are willing to support, not a fast one.** It becomes row 1 of the
   device matrix, and every performance budget in the plan is measured on it (§3).
4. **The purchase order** (§4):

   | Qty | Item | Key requirement |
   |---|---|---|
   | 1 | Reference register | Windows, touchscreen |
   | 1 | 80 mm printer | 576 dots, Ethernet |
   | 1 | 58 mm printer | **384 dots** |
   | 1 | 2D USB scanner | |
   | 1 | Printer-driven cash drawer | |
   |   | Paper and a small switch | |

   Indicative total: **about 550–1,000 JOD**.
5. **Set it up on one LAN, give the agent key-only SSH to the register, and send the handover
   checklist** (§6–§7). From then on the agent qualifies the devices, fills the matrix and runs the
   lab checks. You are needed for the physical acts: reading Arabic on paper, pulling the plug for the
   power-loss drill, and scanning.

---

## 1 · What the hardware unblocks, and what it does not

**Unblocked on arrival:**

| Work | What it needs | Source |
|---|---|---|
| **`1.2.0`'s deferred half**: fill §6a.1 row 1 and `benchmarks/reference-register.toml`, until `--check-profile` exits **0** | the register | [`phase-1-sellable-mvp.md`](implementation/phase-1-sellable-mvp.md) `1.2.0` |
| **`1.7.5`'s paper half**: the seven receipt goldens printed on paper and read by a native reader. The goldens themselves are code, which the agent can write before the hardware arrives | both printers and a reader | `ref/hardware-and-receipts.md` §2.2, §9 check 1 |
| **Printer profiles**: the first 80 mm and 58 mm rows of the §6a matrix | both printers | §6a, *"The first printer row is written in Phase 1"* |
| **Scanner qualification**: the burst heuristic and the Arabic keyboard layout | the scanner | §6a matrix, the scanner row |
| **§9 check 2**: the 58 mm profile prints without truncation | the 58 mm printer | §9 |

**Unblocked as the code arrives**, measured on this hardware and on nothing else:

| Work | Waits on |
|---|---|
| Budgets `1.2.7` (search), `1.4.9` (cart), `1.6.2` (PIN verify), `1.11.13` (scan-to-line) | each step's own code, plus `1.2.0` closed |
| `1.12.3`: the benchmark CI job on `runs-on: [self-hosted, reference-register]` | `1.2.0` closed, and the runner decision in §8 |
| §9 checks 3–6 (the drawer kicks once, and never on a reprint or retry; paper pulled mid-print; printer unplugged at shift open) | the print queue and the finalize: `1.7.6b`, `1.7.7` and `1.8.3` |
| §9 checks 7–8 (the scan kinds, and a scan while the search box has focus) | the scan parser `1.2.4` (blocked by #71) and global scan capture `1.11.6` |
| **Drill E.1b**: real power loss after the receipt printed | the finalize path (`1.8.3`) |
| §9 check 12 (cold start < 3 s) and check 13 (the sale screen read on the register) | the packaged app |

**Not unblocked by hardware.** The cart (`1.4.x`) waits on #71 and #112. The tax engine (`1.3.x`)
waits on #70 and #232. `1.6.2` also waits on #233. §9 checks 9–11 need a payment terminal and a
label printer, which are not on this order (§5), and the full checklist, every class before every
release, arrives at `2.9.4`. Hardware is one of several things between the
project and the Phase-1 exit gate, not the only one.

---

## 2 · What the plan requires of the hardware

Everything in the shopping list follows from this table.

| Requirement | Why | Source |
|---|---|---|
| Printers take **raw ESC/POS**, with **raster `GS v 0`**, over **TCP 9100**, serial or USB. Never webview printing | Arabic is printed as a raster image, the only way it shapes and orders correctly | `ref/hardware-and-receipts.md` §2, §2.1 |
| **576 dots at 80 mm, 384 dots at 58 mm** | the layout's input width; the committed goldens are these widths | §2.1, test case E.49 |
| **A real printer at each width** | *"not one 80 mm unit and an assumption about the other"* | [`merchant-decisions.md`](implementation/ref/merchant-decisions.md) 7.8 |
| **Real-time status** (paper-out, cover-open, offline) | paper-out must warn **before** the money is taken | §3 rule 1, §6a `status_protocol` |
| An **auto-cutter** | the cut (`GS V`) is part of the document's bytes | §6a `cut_command`; `1.7.4` |
| A **drawer port** (`ESC p`) on the printer | the drawer is kicked through the printer by a separate call | §4 |
| Scanner is a **keyboard wedge**, burst under **30 ms** between characters, ending in Enter | that is how a scan is told from typing | §5 |
| Scanner **confirmed against the Arabic keyboard layout** | a wedge scanner types through the OS layout | §6a matrix, the scanner row |
| Register = the **lowest-capability** machine supported | *"the lowest row is what 'the slowest supported hardware' means in every performance budget"* | §6a, §6a.1; [`01-conventions.md`](implementation/01-conventions.md) §7.1 |
| **Not a laptop** | *"A laptop standing in for it is worse than the wait: it makes every budget a number about the wrong computer"* | [`00-master-plan.md`](implementation/00-master-plan.md) §6a |
| Screen at least **1024×640** | the declared minimum window; screenshot baselines are taken at 1024×640 | `tauri.conf.json`; phase-1 `1.11.14` |
| **Touch**, with 48 px or larger hit targets, and no reliance on hover | registers often have no keyboard | [`ui-spec.md`](implementation/ref/ui-spec.md) |
| **Full-disk encryption**, a locked-down kiosk account, **Secure Boot** | the database key is only as safe as the machine's account baseline | [`security-compliance.md`](implementation/ref/security-compliance.md), "the credential store, described accurately" |
| The exact **OS version** recorded | the matrix carries *"the exact Windows, macOS and Linux versions"* supported | §6a matrix, register-OS row |

---

## 3 · Two decisions this settles

### Windows PC, not an Android terminal

#68 says *"the Windows-PC-versus-Android-terminal decision is still unmade"*. The blueprint already
contains the rule
([`plan/engineering-blueprint.md`](plan/engineering-blueprint.md), "Why not the alternatives"):

> **If your primary hardware will be Android smart terminals (Sunmi, iMin, PAX and similar
> all-in-one devices), build in Flutter instead.** … **If your primary hardware is desktop registers
> (Windows/macOS/Linux PCs with USB/serial peripherals), Tauri 2 + Rust is the stronger, leaner
> choice** — and it still gives you Android/iOS as secondary targets later.

The project chose Tauri 2 + Rust and has built 37 microsteps on it. The hardware layer the plan
specifies (`serialport`, `rusb`/`hidapi`, raw ESC/POS over TCP) is a desktop hardware layer. An
Android terminal would exercise almost none of it, so buying one would qualify a platform the Phase-1
plan does not build for. The blueprint keeps Android as a *secondary* target for later; that would be
its own hardware layer and its own qualification, not this purchase. **Recommendation: a Windows
x86-64 register.** x86-64 rather than ARM64,
because the whole toolchain, including the vendored SQLCipher and OpenSSL builds, is standard on
`x86_64-pc-windows-msvc`.

### The lowest machine, on purpose

`01-conventions.md` §7.1 defines the reference register as *"the lowest register-hardware row"*.
Every budget is measured on it: scan-to-line < 100 ms, cart recompute < 16 ms, search < 50 ms over
50,000 SKUs, cold start < 3 s, and PIN verify in its band. A fast machine makes every budget pass
here and fail in the shop. **Buy the slowest machine you would sell to a merchant.** If the pilot
merchant already owns registers, the reference register should be that model or a slower one. Tell
the agent the model, because it changes this choice.

---

## 4 · The shopping list

Prices are **indicative**, converted at the pegged 1 JOD ≈ 1.41 USD, for planning only. Get local
quotes. Buying from an authorised reseller in Jordan is worth a small premium: warranty and swaps
matter more than a discount on lab hardware.

### 4.1 The reference register — 1 unit

A **Windows all-in-one touchscreen POS terminal**, the lowest class you will support.

| Spec | Minimum / choice | Why |
|---|---|---|
| CPU | x86-64, entry class (e.g. Intel N100, or Celeron N5105/J6412) | the floor the budgets are set against |
| RAM | **8 GB** is the recommended floor. Choose 4 GB **only** if you will support 4 GB machines | Windows 11 plus WebView2 on 4 GB is marginal, and whatever you buy is what every budget is promised on |
| Storage | **SSD** (SATA or NVMe), 128 GB or more. Avoid eMMC-only units unless eMMC must be supported | the database commits with `synchronous = FULL` in WAL: every sale is an fsync, and drill E.1b pulls the plug on this disk |
| Screen | 15″ or larger capacitive touch, **1024×768 or more** | above the 1024×640 minimum; 4:3 at 1024×768 is the common low end |
| Security | **TPM 2.0 and Secure Boot** | the security baseline requires full-disk encryption and Secure Boot. Windows 11 Pro needs both anyway, but IoT Enterprise LTSC lists them as *optional*, so check the spec sheet rather than assume ([Microsoft](https://learn.microsoft.com/en-us/windows/iot/iot-enterprise/hardware/system_requirements)) |
| OS | **Windows 11 Pro** or **Windows 11 IoT Enterprise LTSC**. **Not Home** | BitLocker, group policy and kiosk (assigned access) features; Home lacks them. Record the exact version and build |
| Ports | 1 Gb Ethernet; 4 or more USB-A; a COM port is a bonus | printers on Ethernet, scanner on USB, keyboard and mouse for setup |
| Build | fanless preferred | shops are dusty, and it runs all day |

Brands to look at include Posiflex, Partner Tech, Sam4s and Aures, or a local OEM your reseller
supports. What matters is the spec sheet. Indicative cost: **250–450 JOD**.

**Also buy:** a USB keyboard and mouse for setup and development, and a surge-protected power strip.

### 4.2 The 80 mm receipt printer — 1 unit

| Must have | Check on the spec sheet |
|---|---|
| 80 mm paper, **72 mm printable = 576 dots** at 203 dpi | some 80 mm printers print 64 mm (512 dots), which would need a different profile |
| ESC/POS with **raster `GS v 0`** | raster is how Arabic prints |
| **Real-time status** (`DLE EOT`, or Epson's automatic status back) | the paper-out warning before payment |
| **Auto-cutter** | |
| **Drawer kick-out port** (RJ11/RJ12, note the voltage, usually 24 V) | the drawer is driven by this printer |
| **Ethernet** (USB as well is a bonus) | raw TCP 9100 is the simplest transport and reachable from every machine on the LAN |

**Recommended: Epson TM-T20III, Ethernet model.** It is widely stocked, it is Epson's own ESC/POS,
and it prints 72 mm = 576 dots on 80 mm paper (it also takes 58 mm), at 203 dpi, with an auto-cutter,
drawer kick-out, and USB plus serial, parallel or Ethernet depending on the model
([Epson](https://epson.com/For-Work/Printers/POS/TM-T20III-Thermal-Receipt-Printer/p/C31CH51A9972),
[Epson Europe](https://www.epson.eu/en_EU/products/printers/pos-printers/pos-printers/pc-pos-printers/epson-tm-t20iii-series/p/28271),
[Technical Reference Guide](https://files.support.epson.com/pdf/pos/bulk/tm-t20iii_trg_en_reva.pdf)).
The Epson TM-m30III and TM-T88VII are pricier alternatives, and the Bixolon SRP-330/350 series is
another. **Avoid Star printers for this first qualification**: many speak StarPRNT natively, and an
emulation layer is a second thing to qualify. Indicative cost: **130–180 JOD**.

### 4.3 The 58 mm receipt printer — 1 unit

| Must have | Check on the spec sheet |
|---|---|
| 58 mm paper, **48 mm printable = 384 dots** | **this is the one to get right, see below** |
| ESC/POS raster `GS v 0`, **real-time status**, auto-cutter, drawer port | as for 80 mm; many cheap 58 mm units omit the cutter or real status, so ask |
| Ethernet if available, otherwise USB | |

**The width trap.** The plan's 58 mm profile, and the golden `receipt_ar_58mm.bin` that `1.7.5`
commits, are **384 dots** (E.49). Not every 58 mm printer is. The Epson TM-m10 prints **52.5 mm =
420 dots**
([Epson TM-m10 specifications](https://download4.epson.biz/sec_pubs/bs/html/m000920/en/chap10_1.html),
[product guide](https://files.support.epson.com/docid/cpd5/cpd50291.pdf)). A
420-dot device is a *new width*: it needs its own profile, plus a new golden pair with a native-reader
sign-off (§6a *"Qualifying a new device"*, step 4). **Buy a 384-dot (48 mm) printer** so the first 58 mm
row matches the width the plan already specifies. These are common, and two examples show the class on their makers'
pages: the Equip/LevelOne 351001 (48 mm = 384 dots, auto-cutter, drawer port, USB **and Ethernet**)
and the ARKSCAN AS58U (384 dots per line, auto-cutter, RJ-11 drawer port, USB/serial/parallel, no
Ethernet)
([Equip/LevelOne 351001](https://www.level1.com/level1_en/351001-58mm-thermal-pos-receipt-printer-with-auto-cutter-usb-ethernet-cash-drawer-connection-35100107101),
[ARKSCAN AS58U](https://www.arkscan.com/product/shipping-label-printer/thermal-receipt-printer/as58-58mm-receipt-printer)).
Neither page states real-time status support.
It is also what small Jordanian shops tend to run, which is the point of the row. **Ask the seller to
confirm "384 dots per line" and "real-time status (DLE EOT)" in writing.** Indicative cost:
**30–90 JOD**.

### 4.4 The barcode scanner — 1 unit

| Must have | Why |
|---|---|
| **Wired USB, HID keyboard (wedge) mode** | the default mode (§5). Bluetooth wedge timing is less regular, so use a wired unit for qualification |
| **2D area imager** (reads EAN-13/8, UPC-A/E, Code 128, GS1 DataBar **and QR**) | reads damaged labels and phone screens, and the fiscal QR a receipt will carry |
| **Programmable by configuration barcodes**: Enter suffix, no prefix, inter-character delay 0, symbology on/off, **keyboard-country or keypad-emulation** setting | the Arabic-layout qualification (§6.4) may need one of these |
| A stand (presentation mode) | how it is used at a counter |

Examples: Honeywell Voyager 1470g/1450g, Zebra DS2208, Datalogic QuickScan QD2500 series. A no-name
unit may work, but it is the device most likely to fail the burst or layout check, and it rarely comes
with a configuration guide. Indicative cost: **70–130 JOD**.

### 4.5 The cash drawer — 1 unit

A **printer-driven** drawer: RJ11/RJ12 cable, **the same voltage as the 80 mm printer's drawer port**
(usually 24 V), with a coin tray that suits dinar coins. One drawer is enough; it can be moved to the
58 mm printer's port to test that port if the voltages match. Indicative cost: **35–70 JOD**.

### 4.6 Consumables and network

| Item | Note |
|---|---|
| Thermal paper, 80 mm and 58 mm, **BPA-free**, 10 or more rolls each | check the maximum roll diameter each printer takes |
| A small unmanaged gigabit switch, or a spare port on the office router | a lab LAN (§6.1) |
| Cat6 patch cables (4) | printers and the register on Ethernet |
| **~20 real retail products** with barcodes, several of them Jordanian (GS1 prefix **625**) | real labels, real print quality |

Indicative cost: **25–50 JOD**.

### 4.7 The total

| Item | Indicative JOD |
|---|---|
| Reference register | 250–450 |
| 80 mm printer | 130–180 |
| 58 mm printer | 30–90 |
| Scanner | 70–130 |
| Cash drawer | 35–70 |
| Keyboard, mouse, power strip | 15–30 |
| Paper, switch, cables | 25–50 |
| **Total** | **≈ 555–1,000** |

§6a's qualification step 6 asks you to **record what each device cost and where it was bought.** Keep
the receipts; the handover (§7) asks for the figure and the shop.

### 4.8 Ask the seller before paying

- [ ] Printer dot width per line: **576** (80 mm) and **384** (58 mm)
- [ ] ESC/POS **`GS v 0` raster** supported
- [ ] **Real-time status** (`DLE EOT`) supported, and over Ethernet as well as USB
- [ ] Auto-cutter present on **both** printers
- [ ] Drawer port voltage matches the drawer (usually 24 V)
- [ ] Register: **TPM 2.0**, **Secure Boot**, **SSD**, Windows **Pro or IoT Enterprise** (not Home), and the exact edition
- [ ] Scanner: wired USB HID keyboard, 2D, programmable, and its configuration guide available
- [ ] Local warranty and a return window

---

## 5 · Not on this order, and why

| Device | When | Why not now |
|---|---|---|
| **Payment test terminal** | order in Phase 1, use in Phase 2 | It comes **from the acquirer**, not a shop, and needs the conversation first. The plan: *"Acquirer conversation opened, then a physical test terminal — order by Phase 1"* (`00-master-plan.md` §6a). It must be **semi-integrated**: card data never reaches this software (`ref/hardware-and-receipts.md` §6). **Start that conversation now**; it is the longest lead left |
| Label printer | Phase 4 (`4.6.x`) | shelf labels are a compliance feature, but a Phase-4 one (§7) |
| Customer display | Phase 4 and later | §7 |
| Trade scale | only if the pilot sells weighed goods | a trade instrument with JSMO verification (#71). Price-embedded labels are tested with printed samples first |
| A second register | Phase 3 (sync) | the E.12 two-register drill; a second process serves until then |
| UPS | production, per merchant | it would defeat drill E.1b, which must lose power for real |

---

## 6 · Setting it up — arrival day

About half a day for you, then the agent takes over. The layout:

```
                  ┌─────────────────────────── lab LAN (switch / router) ────────────────────────────┐
                  │                          │                          │                          │
          [80 mm printer]            [58 mm printer]          [reference register]         [dev Mac: agent]
          Ethernet, TCP 9100         Ethernet, TCP 9100        Ethernet, SSH :22             Wi-Fi or Ethernet
                  │ RJ11                                          │ USB
            [cash drawer]                                    [barcode scanner]
```

### 6.1 The network 👤

1. Put all four machines on one LAN. **No port forwarding, and nothing exposed to the internet**:
   raw TCP 9100 has no authentication at all.
2. Give each printer and the register a **fixed address**, by DHCP reservation on the router or a
   static IP on the device. Write the addresses down for the handover (§7). They stay out of the
   repository.

### 6.2 The 80 mm printer and the drawer 👤

1. Load 80 mm paper, connect Ethernet, and connect the drawer to the **DK port**.
2. Print the **self-test / status sheet** (the manual shows the button sequence; the Ethernet model
   prints its network settings too). **Photograph it**: it carries the model, firmware and interface
   settings the profile needs.
3. Set its address (Epson's TM utility or its web page), and confirm the paper width setting is
   **80 mm**.
4. **Do not install a Windows printer driver** for it on the register. This product prints raw bytes
   to port 9100 (§2), and a driver adds a spooler path nothing here uses.

### 6.3 The 58 mm printer 👤

The same steps as 6.2, with 58 mm paper. **Check its self-test sheet or specification for "384 dots"**,
and photograph the sheet.

### 6.4 The scanner 👤

1. Plug it into the **register** by USB.
2. Scan its manual's configuration barcodes for **USB HID keyboard**, **suffix: Enter (CR)**, **no
   prefix**, **inter-character delay: minimum**, and enable **EAN-13, EAN-8, UPC-A/E, Code 128, GS1
   DataBar and QR**.
3. **Photograph** which configuration barcodes you scanned, or keep the list. The matrix row records
   the configuration, not only the model.
4. Leave the Arabic-layout test to the agent (§8). It is a qualification check with a record:
   §6a's *"wedge devices confirmed against the burst heuristic and the Arabic keyboard layout"*.

### 6.5 The reference register 👤

1. **A clean OS install** or a factory reset, with all updates applied. Record the **exact** edition,
   version and build (`winver`).
2. **Firmware settings**: Secure Boot **on**, TPM **on**.
3. **BitLocker on** (full-disk encryption is required). Store the recovery key somewhere safe **outside**
   the register and outside the repository.
4. **Time**: zone **(UTC+03:00) Amman**, time set automatically. Jordan is UTC+3 all year, and the
   business-date logic depends on it (phase-1 `1.1.9`).
5. **Keyboards**: install **Arabic (101)** and **English (US)**. The scanner is qualified under both.
6. **Power**: plugged in; sleep **never**; the screen may turn off; note the power mode (for example
   *Balanced*). It is one of the twelve identity fields, and a budget measured in another mode is
   measured on another machine.
7. **Windows Update**: set active hours, and pause updates during benchmark sessions, because a
   background update is a noisy run (`01-conventions.md` §7.1).
8. **Accounts**: your own **admin** account, plus a **standard** account called **`posdev`** for
   builds, benchmarks and drills. A locked-down kiosk account for the app comes later, when the
   product builds kiosk mode. Do not use one account for everything.
9. **No production or merchant data on this machine, ever.** It is a lab.

### 6.6 The toolchain, installed once as admin 👤

Needed to build and run benchmarks, the terminal app and drills **on** the register. Measurements
must run on it, not on the Mac.

- **Git**
- **Visual Studio 2022 Build Tools** with the *Desktop development with C++* workload
- **Strawberry Perl** and **NASM**: the vendored OpenSSL build needs both on Windows
  ([`phase-0-remaining-setup.md`](phase-0-remaining-setup.md), troubleshooting)
- **Python 3**
- **OpenSSH Server** (§6.7)
- **WebView2 Runtime**: present on Windows 11, but check explicitly on LTSC editions

`winget` installs most of these. Check each ID with `winget search <name>` before you rely on it:
`Git.Git`, `Microsoft.VisualStudio.2022.BuildTools`, `StrawberryPerl.StrawberryPerl`, `NASM.NASM`,
`Python.Python.3.12`. The agent installs the per-user tools itself as `posdev` over SSH: `rustup`
(the repository pins Rust through `rust-toolchain.toml`), `mise` for Node **24.19.0** (pinned by
`.nvmrc`, fail-closed), `pnpm`, `just` and `cargo-nextest`.

### 6.7 Give the agent a door: key-only SSH, LAN only 👤

This is what lets the agent work at full speed: reading the machine's identity, building, running
benchmarks and verifying drills without asking you to type commands.

1. **On the Mac**, make a key used for nothing else:
   `ssh-keygen -t ed25519 -f ~/.ssh/pos_register -C "pos-lab"`, then
   `ssh-add --apple-use-keychain ~/.ssh/pos_register`. Use a passphrase; the keychain holds it, so
   the agent's non-interactive `ssh` still works.
2. **On the register, in PowerShell as admin**:
   - Install and start the server:
     `Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0`, then `Start-Service sshd` and
     `Set-Service sshd -StartupType Automatic`.
   - Put the **public** key (`pos_register.pub`) in `C:\Users\posdev\.ssh\authorized_keys`. A
     standard account reads its own file; admin accounts use
     `C:\ProgramData\ssh\administrators_authorized_keys` instead, which is one more reason the agent
     uses `posdev`.
   - In `C:\ProgramData\ssh\sshd_config`, set `PasswordAuthentication no` and restart `sshd`.
   - Restrict the inbound firewall rule for port 22 to the **Private** profile / local subnet.
3. **On the Mac**, add to `~/.ssh/config`:

   ```
   Host pos-register
       HostName <the register's LAN address>
       User posdev
       IdentityFile ~/.ssh/pos_register
   ```

4. **Test it:** `ssh pos-register hostname`. It should print the register's name with no password
   prompt.

**What this grants, stated plainly.** Shell access as a *standard* user, on a lab machine, from the
LAN only. No admin rights, no GitHub credentials (the repository is public and is cloned read-only),
and no merchant data. Anything that needs admin rights you do, or you approve per task. Revoke it by
deleting one line from `authorized_keys`.

---

## 7 · The handover — send this, and the agent takes over

- [ ] **SSH works:** `ssh pos-register hostname` answers. With it, the agent reads the register's CPU,
  RAM, storage, OS build and power mode **itself**, because a measured identity beats a typed one.
- [ ] **The printers:** the addresses of both printers, plus photos of **both self-test sheets**
  (model, firmware, interface).
- [ ] **The scanner:** its model, and the photo or list of the configuration barcodes you scanned.
- [ ] **The drawer:** its model and voltage, and which printer it is plugged into.
- [ ] **For each device**, its cost and where it was bought (§6a qualification step 6).
- [ ] **The native Arabic reader:** who reads the receipts and the register's screen (§9 checks 1
  and 13, and `1.7.5`). If it is you, say so. The reader is named in the dated drill record.
- [ ] **Serial numbers and receipts stay with you.** None of it belongs in the public repository.

---

## 8 · What the agent does after the handover

In this order, each step through the normal pull-request flow and each result recorded where the plan
says:

1. **Qualify the reference register.** Read its identity over SSH, add **row 1** of §6a.1 in
   `ref/hardware-and-receipts.md`, and copy the twelve cells **byte for byte** into
   `benchmarks/reference-register.toml`. Then `python3 scripts/bench-gate.py --check-profile` exits
   **0**, which is the **deferred half of `1.2.0`**, and #68 closes.
2. **Print on paper.** Render the receipt fixtures with `1.7.3`'s rasteriser and `1.7.4`'s emitter,
   and send the bytes straight to each printer's port 9100 from the Mac. No driver is needed and the
   register is not involved. **You read the Arabic.** The agent writes the dated record under
   [`docs/drills/`](drills/README.md), naming the commit, each device and its firmware, and you. That
   is `1.7.5`'s paper half and §9 checks 1 and 2.
3. **Printer profiles.** Write each printer's profile (maker, model, firmware range, transport, dot
   width, raster command, cut, drawer-pulse pin and timings, status protocol, *supports raster
   Arabic*) as the first 80 mm and 58 mm rows of the §6a matrix, each with its qualifying commit.
4. **Scanner qualification.** On the register, in the terminal app, scan real products under **Arabic
   (101) and English (US)** and confirm the burst heuristic sees each as one scan. Alphanumeric codes
   under the Arabic layout are where wedge scanners typically fail. If they do, the fix is the
   scanner's keyboard-country or keypad-emulation setting, and the timing is re-checked afterwards,
   because keypad emulation sends several keystrokes per character. The result is the matrix's scanner
   row.
5. **As each budget microstep lands** (`1.2.7`, `1.4.9`, `1.6.2`, `1.11.13`), measure it **on the
   register** and commit its baseline.
6. **As the print, queue and finalize code lands**, run §9 checks 3–8 and drill **E.1b**. For E.1b
   the agent prepares the sale and the verification, you pull the plug, and the agent checks that
   the sale survived.

**Later, and deliberately not now: the self-hosted CI runner (`1.12.3`).** This repository is
**public**. A self-hosted runner that executes workflow code from pull requests would let anyone who
opens one run code on the register. GitHub's documentation says so directly: *"We recommend that you
only use self-hosted runners with private repositories"*
([GitHub Docs](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/adding-self-hosted-runners)). When `1.12.3` is built, the runner gets:

- its own standard account, and an ephemeral or just-in-time registration;
- a job that runs **only** on `push`, `schedule` or `workflow_dispatch` on protected branches, and
  **never** on `pull_request`;
- no secrets;
- the repository setting that requires approval before workflows run for outside contributors.

Until then, the agent runs benchmarks over SSH.

---

## 9 · Risks and how this setup avoids them

| Risk | Consequence | Avoided by |
|---|---|---|
| Buying a fast register | every budget passes in the lab and fails in the shop | buy the floor (§3) |
| A laptop as the register | *"a number about the wrong computer"* | the plan forbids it (`00-master-plan.md` §6a) |
| A 58 mm printer at 420 dots | a new width, a new golden and a new review | require 384 dots (§4.3) |
| A printer without real-time status | paper-out cannot warn before payment (§3 rule 1) | ask in writing (§4.8) |
| A printer without a cutter | the document's cut command does nothing | require it on both (§4.8) |
| Drawer voltage mismatch | the drawer never opens, or its solenoid is damaged | match the voltage (§4.5) |
| Scanner garbles text under the Arabic layout | wrong codes, and a customer charged for the wrong item | the §8 step-4 check, recorded in the matrix row |
| Windows Home | no BitLocker or kiosk features; security baseline unmet | Pro or IoT Enterprise (§4.1) |
| eMMC storage | slow fsync per sale; power-loss behaviour untested on the disk merchants use | SSD, or a deliberate decision to support eMMC (§4.1) |
| Windows Update mid-benchmark | a noisy run that looks like a regression | active hours and paused updates (§6.5) |
| Printer ports open to the internet | anyone can print, or open the drawer | LAN only (§6.1) |
| A self-hosted runner on a public repository | code execution on the register from a pull request | not before `1.12.3`, and then only with the §8 controls |
| Merchant data on the lab machine | a data-protection incident on a test box | never (§6.5) |

---

## 10 · Decisions needed from you

1. **Platform: a Windows x86-64 register, not Android** (§3). Recommended, following the blueprint's
   own rule.
2. **The floor.** Buy the recommended class, or tell the agent what the pilot merchants run, because
   the reference register should be that or slower.
3. **SSD required**, or eMMC supported (§4.1). SSD is recommended.
4. **SSH access for the agent** as described in §6.7. It is what makes the agent's work after
   arrival fast.
5. **The native Arabic reader** (§7).
6. **Open the acquirer conversation for a test terminal** (§5). It is the longest lead left.

---

## Sources outside this repository

- Epson TM-T20III: [Epson US](https://epson.com/For-Work/Printers/POS/TM-T20III-Thermal-Receipt-Printer/p/C31CH51A9972) ·
  [Epson Europe](https://www.epson.eu/en_EU/products/printers/pos-printers/pos-printers/pc-pos-printers/epson-tm-t20iii-series/p/28271) ·
  [Technical Reference Guide](https://files.support.epson.com/pdf/pos/bulk/tm-t20iii_trg_en_reva.pdf)
- Epson TM-m10, 420 dots at 58 mm: [specifications](https://download4.epson.biz/sec_pubs/bs/html/m000920/en/chap10_1.html) ·
  [TM-m10 and TM-m30 product information guide](https://files.support.epson.com/docid/cpd5/cpd50291.pdf)
- 58 mm, 384-dot devices with a cutter and a drawer port:
  [Equip/LevelOne 351001](https://www.level1.com/level1_en/351001-58mm-thermal-pos-receipt-printer-with-auto-cutter-usb-ethernet-cash-drawer-connection-35100107101) (USB and Ethernet) ·
  [ARKSCAN AS58U](https://www.arkscan.com/product/shipping-label-printer/thermal-receipt-printer/as58-58mm-receipt-printer) (USB, serial, parallel)
- Self-hosted runners on public repositories: [GitHub Docs, "Adding self-hosted runners"](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/adding-self-hosted-runners)
