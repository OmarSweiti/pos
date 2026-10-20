# The hardware lab — what milestone 5.8 buys, how to set it up, and what it qualifies

**Tracks #68.** Written 24 September 2026 against `development` at `1ccdd63`, and **revised on
3 October 2026**: every model below was re-checked against its maker's own documents, prices were
checked with sellers in Amman the same day, and three of the original candidates were replaced
(§4.3, §4.4, §4.1). This is an operator runbook, in the same family as
[`phase-0-remaining-setup.md`](phase-0-remaining-setup.md): it tells a person what to buy and do,
and it hands the agent what it needs to continue. The plan of record is still
[`implementation/`](implementation/). Every requirement below cites the section it comes from,
and where this file and the plan disagree, **the plan is right**.

> **Revised again on 20 October 2026: nothing here is bought before milestone 5.8.** The plan now
> develops every device class against emulators parameterised by device profiles, and buys devices
> at the hardware qualification gate, whose order goes out when Phase 4 begins
> ([`implementation/ref/hardware-and-receipts.md`](implementation/ref/hardware-and-receipts.md) §6b).
> The research below stands — it seeds the profiles and it is the gate's shopping list — but its
> timing does not, and its prices are a 3 October 2026 snapshot to re-quote before ordering.

> **Candidates, not the matrix.** Models named here are examples that meet the plan's requirements
> on their makers' published specifications. **Nothing is supported until it has been bought, held
> and qualified**
> ([`ref/hardware-and-receipts.md`](implementation/ref/hardware-and-receipts.md) §6a: *"Filling it
> with model numbers nobody has held would be the same defect as a compliance claim nobody
> earned."*). Buying a device is step 1 of qualifying it, not the end of it. Prices are what a
> seller showed on 3 October 2026; they move.

Owners are marked 👤 **you** and 🤖 **agent** throughout.

---

## 0 · The answer in one screen

1. **Do not buy yet.** The plan changed on 20 October 2026: devices are bought at the hardware
   qualification gate, milestone 5.8, and the order goes out when Phase 4 begins
   (`ref/hardware-and-receipts.md` §6b, and the master plan's long-lead register, §6a). Until then
   the agent builds the emulators instead (§1), and this file is the gate's research.
2. **At the gate, buy a Windows x86-64 all-in-one touchscreen register, not an Android terminal**, and **never
   one built on an Intel Celeron J1900** (§3).
3. **Buy the *lowest* machine you are willing to support, not a fast one.** It becomes row 1 of the
   device matrix, and every performance budget in the plan is measured on it (§3).
4. **The candidate order** (§4), frozen and re-quoted at `5.8.1`:

   | Qty | Item | Pick | Key requirement |
   |---|---|---|---|
   | 1 | Reference register | Posiflex XT-3815-G2 (Celeron J6412) | Windows 11 IoT Enterprise LTSC, capacitive touch, TPM 2.0 |
   | 1 | 80 mm printer | Epson TM-T20III, Ethernet model | 576 dots, Ethernet |
   | 1 | 58 mm printer | Bixolon SRP-332III with its 2-inch partition | **384 dots** |
   | 1 | 2D USB scanner | Datalogic QuickScan QD2590 | an Arabic keyboard setting |
   | 1 | Printer-driven cash drawer | Posiflex CR-4000-G2 | 24 V |
   |   | Paper, a small switch, cables | | |

   Indicative total: **about 1,290–1,330 JOD** for this set, or **about 790–830 JOD** for what is in
   stock in Amman today, which is a weaker set (§4.7).
5. **When it arrives, set it up on one LAN, give the agent key-only SSH to the register, and send
   the handover checklist** (§6–§7). From then on the agent qualifies the devices, fills the matrix and runs the
   lab checks. You are needed for the physical acts: reading Arabic on paper, pulling the plug for the
   power-loss drill, and scanning.

---

## 1 · What the emulators carry now, and what waits for the gate

**Carried by emulators now** — none of this waits for a device:

| Work | Carried by | Source |
|---|---|---|
| `1.7.9` and `1.7.10`: raster strips at each profile's limit, and the network driver that sends them and reads status back | the emulated network printer (`1.7.12`) and the conformance suite across every printer profile (`1.7.13`) | [`phase-1-sellable-mvp.md`](implementation/phase-1-sellable-mvp.md) |
| `1.7.5`'s review: the seven goldens read by a native reader | each golden's committed PNG, read in the pull request that changes it | `ref/hardware-and-receipts.md` §2.2 |
| The scanner: the burst heuristic and the Arabic keyboard layout | the wedge emulator and its scanner matrix (`1.11.6c`), across the seeded scanner profiles | `1.11.6b`, `1.11.6c` |
| The screen at the registers' resolutions, with touch and without hover | the register-screen profiles (`1.11.14b`) | [`ui-spec.md`](implementation/ref/ui-spec.md) |
| §9 checks 3–10: the drawer kicks once and never on a reprint or retry, paper pulled mid-print, a printer absent at shift open, the scan kinds, the focused search box, every terminal result | the emulated printer, the wedge emulator and the payment simulator `2.1.4` | `ref/hardware-and-receipts.md` §9 |
| The budgets of `1.2.7`, `1.4.9`, `1.6.2`, `1.11.13` and `2.9.3` | their benchmarks, and a relative comparison of every pull request with its base (`1.12.3`). No absolute budget is claimed | [`01-conventions.md`](implementation/01-conventions.md) §7.2 |

**Waits for the gate, milestone 5.8**, on this file's devices and nothing else:

| Work | Needs | Owner |
|---|---|---|
| **`1.2.0`'s deferred half** — row 1 of §6a.1 and `benchmarks/reference-register.toml`, until `--check-profile` exits **0** — then every absolute budget, and `1.12.3`'s live job | the register | `5.8.2` |
| Each device against its profile: the printer rows of the §6a matrix with their raster limits and status behaviour, the drawer, the scanner row under Arabic (101), the terminal — corrected where they disagree, and their captures committed | every device | `5.8.3` |
| The seven goldens **on paper**, read by a native reader; §9 checks 1, 2, 11, 12 and 13; drill **E.1b**, a real power cut; the offline installer on the register | every device | `5.8.4` |

**Not unblocked by hardware at all.** The cart (`1.4.x`) waits on #71 and #112. The tax engine (`1.3.x`)
waits on #70 and #232. `1.6.2` also waits on #233. §9 checks 9 and 10 need the acquirer's terminal and
check 11 a label printer; neither is on this order (§5), and both join the set at `5.8.1`. Hardware is
one of several things between the project and the pre-pilot gate, not the only one.

---

## 2 · What the plan requires of the hardware

Everything in the shopping list follows from this table.

| Requirement | Why | Source |
|---|---|---|
| Printers take **raw ESC/POS**, with a **raster** command, over **TCP 9100**, serial or USB. Never webview printing | Arabic is printed as a raster image, the only way it shapes and orders correctly | `ref/hardware-and-receipts.md` §2, §2.1 |
| **576 dots at 80 mm, 384 dots at 58 mm** | the layout's input width; the committed goldens are these widths | §2.1, test case E.49 |
| **A real printer at each width** | *"not one 80 mm unit and an assumption about the other"* | [`merchant-decisions.md`](implementation/ref/merchant-decisions.md) 7.8 |
| **Real-time status** (paper-out, cover-open, offline), **answered over Ethernet** and not only over serial | paper-out must warn **before** the money is taken | §3 rule 1, §6a `status_protocol` |
| **The raster height limit is known** | one raster command carries a limited number of dot rows (2303 on the Epson TM-T20III); a longer receipt is sent in strips at that limit | §2.1, `1.7.9` |
| An **auto-cutter** | the cut (`GS V`) is part of the document's bytes | §6a `cut_command`; `1.7.4` |
| A **drawer port** (`ESC p`) on the printer, at the **drawer's voltage** (usually 24 V) | the drawer is kicked through the printer by a separate call | §4 |
| **Ethernet** on both printers | raw TCP 9100 needs no driver anywhere; Epson's and Star's USB default to a vendor-specific class that needs one ([Epson TM-T88VII TRG](https://files.support.epson.com/pdf/pos/bulk/tm-t88vii_trg_en_reva.pdf)) | `1.7.10` |
| Scanner is a **keyboard wedge**, burst under **30 ms** between characters, ending in Enter | that is how a scan is told from typing | §5 |
| Scanner **confirmed against the Arabic keyboard layout** | a wedge scanner types through the OS layout | §6a matrix, the scanner row; `1.11.6b` |
| Register = the **lowest-capability** machine supported | *"the lowest row is what 'the slowest supported hardware' means in every performance budget"* | §6a, §6a.1; [`01-conventions.md`](implementation/01-conventions.md) §7.1 |
| **Not a laptop** | *"A laptop standing in for it is worse than the wait: it makes every budget a number about the wrong computer"* | [`00-master-plan.md`](implementation/00-master-plan.md) §6a |
| Screen at least **1024×640** | the declared minimum window; screenshot baselines are taken at 1024×640 | `tauri.conf.json`; phase-1 `1.11.14` |
| **Capacitive touch**, with 48 px or larger hit targets, and no reliance on hover | registers often have no keyboard, and a resistive panel needs a press, not a tap | [`ui-spec.md`](implementation/ref/ui-spec.md) |
| **Full-disk encryption**, a locked-down kiosk account, **Secure Boot** | the database key is only as safe as the machine's account baseline | [`security-compliance.md`](implementation/ref/security-compliance.md), "the credential store, described accurately" |
| The exact **OS version** recorded | the matrix carries *"the exact Windows, macOS and Linux versions"* supported | §6a matrix, register-OS row |

---

## 3 · Three decisions this settles

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
Android terminal would exercise almost none of it: Sunmi's and iMin's built-in printers are reached
through the maker's print service or a virtual Bluetooth printer, with vendor-specific drawer bytes
([Sunmi developer document](https://cdn.sunmi.com/public/generalfile/mgt-document/841c6680d673447ba9c5d9b1e1131d01.pdf)),
so buying one would qualify a platform the Phase-1 plan does not build for. The blueprint keeps
Android as a *secondary* target for later; that would be its own hardware layer and its own
qualification, not this purchase. **Recommendation: a Windows x86-64 register.** x86-64 rather
than ARM64, because the whole toolchain, including the vendored SQLCipher and OpenSSL builds, is
standard on `x86_64-pc-windows-msvc`.

### The lowest machine, on purpose

`01-conventions.md` §7.1 defines the reference register as *"the lowest register-hardware row"*.
Every budget is measured on it: scan-to-line < 100 ms, cart recompute < 16 ms, search < 50 ms over
50,000 SKUs, cold start < 3 s, and PIN verify in its band. A fast machine makes every budget pass
here and fail in the shop. **Buy the slowest machine you would sell to a merchant.** A Celeron
J6412 is a sound floor; an Intel N100 is noticeably faster, so an N100 register makes the budgets
easier to pass than the shop will. If the pilot merchant already owns registers, the reference
register should be that model or a slower one. Tell the agent the model, because it changes this
choice.

### Which processor, and which Windows

Many cheap terminals sold in Amman are built on old Celeron chips. Microsoft publishes which
processors each Windows edition supports, and the answer is not the same for every edition:

| Processor | Family, launch | Windows 11 Pro / Home | Windows 11 IoT Enterprise LTSC 2024 | Verdict |
|---|---|---|---|---|
| Celeron **J1900** | Bay Trail, 2013 | not listed | not listed | **never buy** |
| Celeron J4125 | Gemini Lake Refresh, 2019 | listed | not listed | Pro only; no LTSC |
| Celeron **J6412** | Elkhart Lake, 2021 | not listed | listed | **IoT Enterprise only** |
| Celeron N5105 | Jasper Lake, 2021 | listed | listed | either |
| Intel **N100** / N97 | Alder Lake-N, 2023 | listed | listed | either |

Sources: Microsoft's [Windows 11 25H2 supported Intel processors](https://learn.microsoft.com/en-us/windows-hardware/design/minimum/supported/windows-11-25h2-supported-intel-processors)
and [Windows 11 IoT Enterprise supported Intel processors](https://learn.microsoft.com/en-us/windows/iot/iot-enterprise/hardware/supported/winiot_intel_processors_2024).
Windows 11 IoT Enterprise LTSC 2024 relaxes TPM, Secure Boot and UEFI to *optional* but **not** the
processor list ([system requirements](https://learn.microsoft.com/en-us/windows/iot/iot-enterprise/hardware/system_requirements)).
It is serviced until **10 October 2034**; Windows 10 IoT Enterprise LTSC 2021 until **13 January
2032**; ordinary Windows 10 support ended on **14 October 2025**
([lifecycle](https://learn.microsoft.com/en-us/lifecycle/products/windows-11-iot-enterprise-ltsc-2024)).
Windows 7 and POSReady 7 cannot run this product at all: WebView2 stopped at version 109 on those
systems in January 2023
([Microsoft Edge blog](https://blogs.windows.com/msedgedev/2022/12/09/microsoft-edge-and-webview2-ending-support-for-windows-7-and-windows-8-8-1/)),
and terminals sold in Amman still ship that way: a GSAN GS-T3 listed at 590 JOD pairs a J1900 with
Windows 7 ([GTS](https://gts.jo/en/gsan-gs-t3-touch-pos-system-all-in-one-quad-core)).

---

## 4 · The shopping list

Prices are **indicative**, as a seller showed them on 3 October 2026, and they will be re-quoted at
`5.8.1`: models and prices both move in the months before the gate. US prices are converted at
the pegged 1 JOD ≈ 1.41 USD, before shipping, customs and tax. Get local quotes. Buying from an
authorised reseller in Jordan is worth a small premium: warranty and swaps matter more than a
discount on lab hardware.

### 4.1 The reference register — 1 unit

A **Windows all-in-one touchscreen POS terminal**, the lowest class you will support.

| Spec | Minimum / choice | Why |
|---|---|---|
| CPU | x86-64, entry class: **Celeron J6412** (IoT Enterprise only), N5105, or N100/N97. **Never J1900** | the floor the budgets are set against; §3 |
| RAM | **8 GB** is the recommended floor. Choose 4 GB **only** if you will support 4 GB machines | Windows 11 plus WebView2 on 4 GB is marginal, and whatever you buy is what every budget is promised on |
| Storage | **SSD** (SATA or NVMe), 128 GB or more. Avoid eMMC-only units unless eMMC must be supported | the database commits with `synchronous = FULL` in WAL: every sale is an fsync, and drill E.1b pulls the plug on this disk |
| Screen | 15″ or larger **capacitive (PCAP)** touch, **1024×768 or more** | above the 1024×640 minimum; 4:3 at 1024×768 is the common low end |
| Security | **TPM 2.0 and Secure Boot** | the security baseline requires full-disk encryption and Secure Boot. Windows 11 Pro needs both anyway, but IoT Enterprise LTSC lists them as *optional*, so check the spec sheet rather than assume |
| OS | **Windows 11 Pro** or **Windows 11 IoT Enterprise LTSC**. **Not Home** | BitLocker, group policy and kiosk (assigned access) features; Home lacks them. Record the exact version and build |
| Ports | 1 Gb Ethernet; 4 or more USB-A; a COM port is a bonus | printers on Ethernet, scanner on USB, keyboard and mouse for setup |
| Build | fanless preferred | shops are dusty, and it runs all day |

**Recommended: [Posiflex XT-3815-G2](https://www.posiflex.com/product-detail/xt-3815-g2/).**
Celeron J6412, two DDR4 SO-DIMM slots (up to 32 GB), M.2 SATA storage, **TPM 2.0**, Windows 10 or 11
IoT LTSC, a 15″ 1024×768 **true-flat PCAP multi-touch** panel, gigabit LAN, 3× USB 3.0 + 4× USB 2.0,
3× DB9 serial + RJ50, an RJ11 drawer port for a 24 V drawer, and **fan-free**. A US reseller lists the 8 GB / 128 GB SSD configuration at **US$1,103.99
(≈ 783 JOD)**
([CDW](https://www.cdw.com/product/posiflex-xt-series-xt-3815-g2-all-in-one-celeron-j6412-2-ghz-8-gb-s/8448759));
that SKU ships Windows 11 IoT Enterprise on the general channel, so **order the LTSC build**. Ask
PC Stores in Amman, a Posiflex reseller, for a local quote. Partner Tech's Grace G5 (J6412,
fanless, Windows 11 IoT LTSC) and Audrey A4 II (N97, Windows 11 IoT LTSC 24H2) are alternatives
by quote from Partner Tech Middle East.

**Budget, in stock in Amman: MSI PRO AP162T** — Intel N100, 8 GB, 128 GB NVMe, Windows 11 Pro, a
15.6″ 1920×1080 panel, 2× LAN, 6× USB-A, 2× COM — **399 JOD** at
[City Center](https://citycenter.jo/electronics-point-sale/msi-pro-ap162t-2024-new-12gen-intel-celeron-n100-4-cores-w-15-6-ips-full-hd-anti-glare-touch-display-black)
with a one-year warranty. Two costs come with the price: its touch panel is **"Resistive Single
Touch"** ([MSI specification](https://www.msi.com/All-in-One-PC/PRO-AP162T-ADL/Specification)),
which needs a press rather than a tap, and the N100 is faster than the J6412 floor (§3). It is a
workable stop-gap, not the reference register.

**Also buy:** a USB keyboard and mouse for setup and development, and a surge-protected power strip.

### 4.2 The 80 mm receipt printer — 1 unit

| Must have | Check on the spec sheet |
|---|---|
| 80 mm paper, **72 mm printable = 576 dots** at 203 dpi | some 80 mm printers print 64 mm (512 dots), and 180 dpi models print 512 dots across 72 mm; either would need a different profile |
| ESC/POS with a **raster** command (`GS v 0`, or `GS ( L`) and its **height limit** | raster is how Arabic prints; the limit decides the strip size (`1.7.9`) |
| **Real-time status** (`DLE EOT`, or Epson's automatic status back) **over Ethernet** | the paper-out warning before payment |
| **Auto-cutter** | |
| **Drawer kick-out port** (RJ11/RJ12, note the voltage, usually 24 V) | the drawer is driven by this printer |
| **Ethernet** (USB as well is a bonus) | raw TCP 9100 is the simplest transport and reachable from every machine on the LAN |

**Recommended: Epson TM-T20III, Ethernet model (C31CH51012) — 192.67 JOD at
[Jordan Computers](https://jordancomputers.com/en/tm-t20iii-012-en).** It is Epson's own ESC/POS,
prints 72 mm = 576 dots at 203 dpi, has an auto-cutter and a 6-pin drawer port supplying 24 V on
pin 4, and its Ethernet model exchanges *"printing data and printer status via (bidirectional)
direct socket communication"* — status over the network, which is the requirement
([Technical Reference Guide](https://files.support.epson.com/pdf/pos/bulk/tm-t20iii_trg_en_reva.pdf)).
Epson Europe now lists this SKU as
[discontinued](https://www.epson.eu/en_EU/products/printers/pos-printers/pos-printers/pc-pos-printers/epson-tm-t20iii-%28012%29%3A-ethernet%2C-ps%2C-blk%2C-eu/p/28150); its successor, the **TM-T20IV** (C31CL47002: USB,
serial and Ethernet), prints the same 576 dots and qualifies equally
([Epson](https://news.epson.com/news/tm-t20iv-thermal-pos-printer)). Buy whichever is in stock.

Three facts about it the lab must know:

- **Do not convert it to 58 mm.** It takes 58 mm paper with an included guide, but there it prints
  52.5 mm = **420 dots**, not 384, and *"once you change the paper width from 80 to 58 mm … and use
  the printer, you cannot change it back to 80 mm"* (Technical Reference Guide). The lab needs it
  at 80 mm for its whole life.
- **One raster command carries at most 2303 dot rows** on the TM-T20III, TM-T20IV and TM-m30III
  (4095 on the TM-T88VII), and Epson marks `GS v 0` *"[obsolete command]"* that *"will not be
  supported by future models"*, recommending `GS ( L`
  ([Epson ESC/POS reference](https://download4.epson.biz/sec_pubs/pos/reference_en/escpos/gs_lv_0.html)).
  2303 rows is about 29 cm of receipt. `1.7.9` sends a longer receipt in strips; until it lands,
  paper tests use receipts shorter than that.
- **The TM-T88VII is not a drop-in alternative here**: it is 180 dpi, so it prints 512 dots on
  80 mm paper, a different width from the goldens.

**Avoid Star printers for this first qualification**: most speak StarPRNT natively, and an emulation
layer is a second thing to qualify. Star added ESC/POS to some TSP143IV models in January 2026
([Star](https://starmicronics.com/blog/star-micronics-expands-esc-pos-printer-compatibility-across-thermal-and-impact-printing-solutions/));
the mC-Print2 and mC-Print3 remain StarPRNT only. **Cheap Xprinter models are common in Amman** and
worth a later row, but Xprinter's own 80-series programming manual marks `DLE EOT` as valid only
on serial printers ([XP-N160II](https://www.xprinter.net/product/508.html)), so one on Ethernet
cannot warn before payment.

### 4.3 The 58 mm receipt printer — 1 unit

| Must have | Check on the spec sheet |
|---|---|
| 58 mm paper, **48 mm printable = 384 dots** | **this is the one to get right, see below** |
| ESC/POS raster, **real-time status**, auto-cutter, drawer port **at 24 V** | as for 80 mm; many cheap 58 mm units omit the cutter, the status, or the 24 V port |
| Ethernet if available, otherwise USB | |

**The width trap.** The plan's 58 mm profile, and the golden `receipt_ar_58mm.bin` that `1.7.5`
commits, are **384 dots** (E.49). Most 58 mm modes are not. The Epson TM-m10 prints 52.5 mm =
**420 dots**
([Epson TM-m10 specifications](https://download4.epson.biz/sec_pubs/bs/html/m000920/en/chap10_1.html));
so do the TM-T20III, TM-T20IV and TM-m30III in their 58 mm mode, and the 180 dpi TM-T88VII prints
360. A 420-dot device is a *new width*: it needs its own profile, plus a new golden pair with a
native-reader sign-off (§6a *"Qualifying a new device"*, step 4).

**The two candidates this file first named do not meet the requirement.** The Equip/LevelOne
351001's own manual contradicts itself on the drawer voltage (12 V in its specification table,
24 V on page 10) and documents no status command
([manual](https://www.manua.ls/equip/351001/manual)). The ARKSCAN AS58U has a 12 V drawer port,
no Ethernet, and drivers listed only up to Windows 7/8
([ARKSCAN](https://www.arkscan.com/product/shipping-label-printer/thermal-receipt-printer/as58-58mm-receipt-printer)).

**Recommended: Bixolon SRP-332III**, the **203 dpi** member of the SRP-330III series (the
SRP-330III itself is 180 dpi; do not buy it), **with the 2-inch partition that comes in the box.**
At 8 dots/mm its printing width is **72 mm on 3-inch paper and 48 mm on 2-inch paper = 384 dots**
([user manual](https://bixolon.com/_upload/manual/Manual_User_SRP-33xIII_ENG_V1.00.pdf)). The
interface set *"USB 2.0 FS + Ethernet + Serial"*, an auto-cutter, a 24 V drawer port, and the
partition are on its
[specification sheet](https://bixoloneu.com/wp-content/uploads/2025/04/SS_SRP-330III_EN_JAN25_V1.pdf);
Bixolon's command manual documents `DLE EOT` and network printing on port 9100, though not, in so
many words, status over Ethernet — ask. About **US$165 (≈ 117 JOD)** in the US
([TouchWindow](https://www.touchwindow.com/p/SRP-332IIIESK.html)); no Jordanian listing was found,
so ask a Bixolon dealer. A second brand is also what tests "ESC/POS is a family, not a standard"
(§6a) on day one.

**Like the Epson, it is one-way:** its manual says *"Do not use 3-inch paper after using 2-inch
paper"*. So qualify it at **80 mm first** — a second 576-dot profile from a second maker — and then
fit the partition and keep it at 58 mm for good. Backup: the HPRT TP585, a native 58 mm printer with
a cutter and a 24 V drawer port but USB only and undocumented status
([HPRT](https://www.hprt.com/Product/TP585-2-inch-Thermal-Receipt-Printer-TP585-585L.html)).
**Ask the seller to confirm "384 dots per line", "real-time status over Ethernet" and the drawer
voltage in writing.**

### 4.4 The barcode scanner — 1 unit

| Must have | Why |
|---|---|
| **Wired USB, HID keyboard (wedge) mode** | the default mode (§5). Bluetooth wedge timing is less regular, so use a wired unit for qualification |
| **2D area imager** (reads EAN-13/8, UPC-A/E, Code 128, GS1 DataBar **and QR**) | reads damaged labels and phone screens, and the fiscal QR a receipt will carry |
| **Programmable by configuration barcodes**: Enter suffix, no prefix, inter-character delay 0, symbology on/off, and a documented **Arabic keyboard-country** or **ALT/keypad-emulation** setting | the Arabic-layout qualification (§6.4) may need one of these |
| A stand (presentation mode) | how it is used at a counter |

**Recommended: Datalogic QuickScan QD2590 — 85 JOD, in stock, five-year warranty, at
[GTS Amman](https://gts.jo/en/datalogic/datalogic-quickscan-qd2590-bk-corded-handheld-omnidirectional-area-imager-barcode-scanner-black-1d-2d-and-postal-codes).**
Its product reference guide documents country modes *Arabic 101*, *Arabic 102* and *Arabic 102
AZERTY*, an *"alternative key encoding"* keyboard interface, and an ALT output type
([Datalogic](https://cdn.datalogic.com/Download?iddwnfile=36041)). Confirm the stand is in the box;
the QD2590-BKK1S kit includes one. **Backup: Zebra DS2208**, whose guide documents *"Arabic (101)"*
as a keyboard country and keypad emulation
([Zebra](https://www.zebra.com/content/dam/support-dam/en/documentation/unrestricted/guide/product/ds2208-prg-en.pdf));
about US$166 (≈ 118 JOD), no Jordanian price found.

**No longer recommended: Honeywell Voyager 1470g.** Its user guide lists keyboard countries without
Arabic and has no ALT mode, so it cannot be configured around the layout problem, and some
"1470g" listings in Amman name another brand. The software side is `1.11.6b`: it decodes a scan by
physical key, so a scanner left on its default US layout works under Arabic (101) as well.

### 4.5 The cash drawer — 1 unit

A **printer-driven** drawer with an RJ11/RJ12 cable, **the same voltage as the 80 mm printer's drawer
port** (24 V), and coin cups for the dinar's coins: ½ JD, ¼ JD, 100, 50, 25, 10 and 5 fils, seven
in all ([Ministry of Foreign Affairs](https://www.mfa.gov.jo/content/official-currency)).

**Recommended: Posiflex CR-4000-G2** — *"Printer driven, powered by printer or Posiflex system CR
port, support 12V/24V voltage range"*, five adjustable note compartments and nine coin cups
([Posiflex](https://www.posiflex.com/product-detail/cr-4000-g2-series_4100-g2-series/)). PC Stores in
Amman lists a "POSIFLEX Cash Drawer PP-4000" at **70 JOD**
([PC Stores](https://pcstoresfutec.com/shop)); confirm it is this printer-driven model, because the
listing says eight coin cups. Backup: APG Vasario VB320, a 24 V printer-driven drawer whose RJ45
jack takes the included Epson cable, about US$147
([POSGuys](https://posguys.com/cash-drawer_32/APG-Vasario-Series_61/)).

Before the first kick: the cable must be a **6-wire RJ12**; Epson requires the solenoid to be
**24 Ω or more** (or to draw 1 A or less), and no drawer maker publishes its coil resistance, so
**measure it**; and whether the drawer fires on pin 2 or pin 5 is a property of the drawer, recorded
in the printer profile (§6a). One drawer is enough; it can move to the 58 mm printer's port to test
that port.

### 4.6 Consumables and network

| Item | Note |
|---|---|
| Thermal paper, 80 mm and 58 mm, **BPA-free**, 10 or more rolls each | check the maximum roll diameter each printer takes (80 mm on the Bixolon) |
| A small unmanaged gigabit switch, or a spare port on the office router | a lab LAN (§6.1) |
| Cat6 patch cables (4) | printers and the register on Ethernet |
| **~20 real retail products** with barcodes, several of them Jordanian (GS1 prefix **625**), and at least one **alphanumeric** code (a Code 128 shipping label, a QR) | real labels, real print quality, and the layout test (§8 step 4) |

Indicative cost: **25–50 JOD**.

### 4.7 The total

| Item | Indicative JOD |
|---|---|
| Reference register: Posiflex XT-3815-G2 | ≈ 783 (US price) — or MSI PRO AP162T, 399 (Amman) |
| 80 mm printer: Epson TM-T20III Ethernet | 192.67 (Amman) |
| 58 mm printer: Bixolon SRP-332III | ≈ 117 (US price) |
| Scanner: Datalogic QD2590 | 85 (Amman) |
| Cash drawer: Posiflex CR-4000-G2 | 70 (Amman) |
| Keyboard, mouse, power strip | 15–30 |
| Paper, switch, cables | 25–50 |
| **Total, the recommended set** | **≈ 1,290–1,330**, plus shipping and customs on the two imported items |
| **Total, in stock in Amman today** (MSI register, no 58 mm printer) | **≈ 790–830**, with no 384-dot row and a resistive panel |

The first figure in this file was **≈ 555–1,000**. It assumed a register at 250–450 JOD; a branded
J6412 terminal with capacitive touch and a TPM costs more than that, and the only locally stocked
machine near that price is the resistive MSI. §6a's qualification step 6 asks you to **record what
each device cost and where it was bought.** Keep the receipts; the handover (§7) asks for the figure
and the shop.

### 4.8 Ask the seller before paying

- [ ] Printer dot width per line: **576** (80 mm) and **384** (58 mm), at the paper width you will use
- [ ] ESC/POS raster (`GS v 0` or `GS ( L`) supported, and the most dot rows one command accepts
- [ ] **Real-time status** (`DLE EOT`) supported, and **over Ethernet** as well as USB or serial
- [ ] Auto-cutter present on **both** printers
- [ ] Whether the printer can return to 80 mm after running 58 mm paper (the two above cannot)
- [ ] Drawer port voltage matches the drawer (24 V); the cable is a 6-wire RJ12
- [ ] Register: the **exact CPU** (never J1900), **TPM 2.0**, **Secure Boot**, **SSD**, a
      **capacitive (PCAP)** panel, Windows **Pro or IoT Enterprise** (not Home), and the exact edition.
      A J6412 must come with **IoT Enterprise**
- [ ] Scanner: wired USB HID keyboard, 2D, programmable, an Arabic keyboard-country or ALT setting,
      and its configuration guide available
- [ ] Local warranty and a return window

---

## 5 · Not on this order, and why

| Device | When | Why not now |
|---|---|---|
| **Payment test terminal** | ask in Phase 1, qualify at 5.8 | It comes **from the acquirer**, not a shop, and needs the conversation first. The plan: the conversation in Phase 1, the terminal delivered for milestone 5.8, and Phase 2 built against the simulator and the acquirer's test host (`00-master-plan.md` §6a, `phase-2-money-grade.md` 2.1.1). It must be **semi-integrated**: card data never reaches this software (`ref/hardware-and-receipts.md` §6). **Start that conversation now**; it is the longest lead left |
| **An Android POS terminal** (Sunmi, iMin, PAX) | only if a pilot merchant insists | its built-in printer is reached through the maker's service or a virtual Bluetooth printer, not raw TCP; that is a second hardware layer, not a row in this one (§3) |
| Label printer | at 5.8, with the rest | shelf labels are a compliance feature and Phase 4's exit prints them, so one joins the frozen set at `5.8.1` (§7) |
| Customer display | at 5.8, only if a pilot store has one | §7. A 5×7 dot-matrix pole display cannot show Arabic; a second screen can. Until then a profile and a second window stand in |
| Trade scale | only if the pilot sells weighed goods | a trade instrument with JSMO verification (#71). Price-embedded labels are tested with printed samples first |
| A second register | not bought for Phase 3 | the E.12 two-register drill runs on a second virtual machine; physical registers come with the gate |
| UPS | production, per merchant | it would defeat drill E.1b, which must lose power for real |

---

## 6 · Setting it up — arrival day, at milestone 5.8

About half a day for you, then the agent takes over. The layout:

```
                  ┌─────────────────────────── lab LAN (switch / router) ────────────────────────────┐
                  │                          │                          │                          │
          [80 mm printer]            [58 mm printer]          [reference register]         [dev Mac: agent]
          Ethernet, TCP 9100         Ethernet, TCP 9100        Ethernet, SSH :22             Wi-Fi or Ethernet
                  │ RJ12                                          │ USB
            [cash drawer]                                    [barcode scanner]
```

### 6.1 The network 👤

1. Put all four machines on one LAN. **No port forwarding, and nothing exposed to the internet**:
   raw TCP 9100 has no authentication at all.
2. Give each printer and the register a **fixed address**, by DHCP reservation on the router or a
   static IP on the device. Write the addresses down for the handover (§7). They stay out of the
   repository.

### 6.2 The 80 mm printer and the drawer 👤

1. Load 80 mm paper, connect Ethernet, and connect the drawer to the **DK port**. **Measure the
   drawer's solenoid first** (§4.5).
2. Print the **self-test / status sheet** (the manual shows the button sequence; the Ethernet model
   prints its network settings too). **Photograph it**: it carries the model, firmware and interface
   settings the profile needs.
3. Set its address (Epson's TM utility or its web page), and confirm the paper width setting is
   **80 mm**. Do not fit the 58 mm guide (§4.2).
4. **Do not install a Windows printer driver** for it on the register. This product prints raw bytes
   to port 9100 (§2), and a driver adds a spooler path nothing here uses.

### 6.3 The 58 mm printer 👤

The same steps as 6.2. Qualify it at **80 mm first** (§8 step 3), then fit the **2-inch partition**,
load 58 mm paper, and leave it at 58 mm for good (§4.3). **Check its self-test sheet or
specification for "384 dots"** at 58 mm, and photograph the sheet at both widths.

### 6.4 The scanner 👤

1. Plug it into the **register** by USB.
2. Scan its manual's configuration barcodes for **USB HID keyboard**, **suffix: Enter (CR)**, **no
   prefix**, **inter-character delay: minimum**, and enable **EAN-13, EAN-8, UPC-A/E, Code 128, GS1
   DataBar and QR**. Leave the keyboard country on its default (US): `1.11.6b` decodes by physical key.
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
- **WebView2 Runtime**: present on Windows 11, but check explicitly on LTSC editions. The shipped
  installer will carry its own (`2.9.5b`); the lab register needs it now

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
  (model, firmware, interface), and the 58 mm printer's sheet at each width.
- [ ] **The scanner:** its model, and the photo or list of the configuration barcodes you scanned.
- [ ] **The drawer:** its model, voltage, measured solenoid resistance, and which printer it is
  plugged into.
- [ ] **For each device**, its cost and where it was bought (§6a qualification step 6).
- [ ] **The native Arabic reader:** who reads the receipts and the register's screen (§9 checks 1
  and 13, and `1.7.5`). If it is you, say so. The reader is named in the dated drill record.
- [ ] **Serial numbers and receipts stay with you.** None of it belongs in the public repository.

---

## 8 · What the agent does after the handover

In this order, each step through the normal pull-request flow and each result recorded where the plan
says. These are milestone 5.8's microsteps, `5.8.2` to `5.8.4`:

1. **Qualify the reference register.** Read its identity over SSH, add **row 1** of §6a.1 in
   `ref/hardware-and-receipts.md`, and copy the twelve cells **byte for byte** into
   `benchmarks/reference-register.toml`. Then `python3 scripts/bench-gate.py --check-profile` exits
   **0**, which is the **deferred half of `1.2.0`**, and #68 closes.
2. **Print on paper.** Render the receipt fixtures with `1.7.3`'s rasteriser and `1.7.4`'s emitter,
   and send the bytes straight to each printer's port 9100 from the Mac. No driver is needed and the
   register is not involved. Until `1.7.9` sends long receipts in strips, keep each test receipt under
   **2303 dot rows** (§4.2). **You read the Arabic.** The agent writes the dated record under
   [`docs/drills/`](drills/README.md), naming the commit, each device and its firmware, and you. That
   is `1.7.5`'s paper half and §9 checks 1 and 2.
3. **Printer profiles, confirmed or corrected.** Run the conformance suite (`1.7.13`) against each
   printer with its seeded profile (maker, model, firmware range, transport, dot width, raster
   command and its row limit, cut, drawer-pulse pin and timings, status protocol and whether it
   answers over Ethernet, *supports raster Arabic*). Where the printer disagrees, write a new profile
   version and commit the capture; then add the first 80 mm and 58 mm rows of the §6a matrix, each
   with its qualifying commit. The Bixolon gets an 80 mm row before it is
   converted to 58 mm.
4. **Scanner qualification.** On the register, in the terminal app, scan real products under **Arabic
   (101) and English (US)** and confirm the burst heuristic sees each as one scan. Alphanumeric codes
   under the Arabic layout are where wedge scanners typically fail; `1.11.6b` is the software answer,
   and the scanner's keyboard-country or ALT setting is the device answer, re-timed afterwards
   because ALT/keypad emulation sends several keystrokes per character. The result is the matrix's
   scanner row.
5. **Measure every budget on the register** (`1.2.7`, `1.4.9`, `1.6.2`, `1.11.13`, `2.9.3`) and commit
   each baseline. The benchmarks exist by then; only the machine was missing.
6. **Run §9 on the devices** — checks 3–8 have run against the emulators since `2.9.4` and now run
   on paper — and drill **E.1b**. For E.1b the agent prepares the sale and the verification, you pull
   the plug, and the agent checks that the sale survived.

**The self-hosted CI runner — `1.12.3`'s live job, switched on at `5.8.2`.** This repository is
**public**. A self-hosted runner that executes workflow code from pull requests would let anyone who
opens one run code on the register. GitHub's documentation says so directly: *"We recommend that you
only use self-hosted runners with private repositories"*
([GitHub Docs](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/adding-self-hosted-runners)). When `1.12.3` is built, the runner gets:

- its own standard account, and an ephemeral or just-in-time registration;
- a job that runs **only** on `push`, `schedule` or `workflow_dispatch` on protected branches, and
  **never** on `pull_request`;
- no secrets;
- the repository setting that requires approval before workflows run for outside contributors.

Until it is registered, the agent runs benchmarks over SSH.

---

## 9 · Risks and how this setup avoids them

| Risk | Consequence | Avoided by |
|---|---|---|
| Buying before the profiles and the pilot are known | a floor no merchant runs, or a width no store uses | buy at milestone 5.8, against the frozen matrix (§0) |
| Buying a fast register | every budget passes in the lab and fails in the shop | buy the floor (§3) |
| A J1900 register | no supported Windows 11 edition at all | the processor table (§3) |
| A J6412 register with Windows 11 Pro | an edition Microsoft does not list for that processor | order IoT Enterprise (§3, §4.8) |
| A laptop as the register | *"a number about the wrong computer"* | the plan forbids it (`00-master-plan.md` §6a) |
| A resistive-touch register | taps need pressure; not the register the UI is designed for | capacitive (§4.1) |
| A 58 mm printer at 420 dots | a new width, a new golden and a new review | require 384 dots (§4.3) |
| Converting the 80 mm Epson to 58 mm | permanent, and 420 dots | keep it at 80 mm (§4.2) |
| A printer without real-time status, or with it on serial only | paper-out cannot warn before payment (§3 rule 1) | ask in writing (§4.8) |
| A receipt taller than the printer's raster limit | the printer refuses or truncates it | strips at the profile's limit (`1.7.9`); short test receipts until then (§8) |
| A USB-only printer | Epson's and Star's USB default to a vendor class nothing here drives yet | Ethernet (§2) |
| A printer without a cutter | the document's cut command does nothing | require it on both (§4.8) |
| Drawer voltage mismatch, or a low-resistance solenoid | the drawer never opens, or its solenoid or the printer's driver is damaged | match the voltage and measure (§4.5) |
| Scanner garbles text under the Arabic layout | wrong codes, and a customer charged for the wrong item | `1.11.6b`, and the §8 step-4 check recorded in the matrix row |
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
2. **The register**: the Posiflex XT-3815-G2 (recommended; ≈ 783 JOD imported, or a local quote) or
   the MSI PRO AP162T (399 JOD in Amman, resistive touch, a faster floor) (§4.1). Or tell the agent
   what the pilot merchants run, because the reference register should be that or slower.
3. **SSD required**, or eMMC supported (§4.1). SSD is recommended.
4. **SSH access for the agent** as described in §6.7. It is what makes the agent's work after
   arrival fast.
5. **The native Arabic reader** (§7).
6. **Open the acquirer conversation now** (§5); the terminal itself is needed at milestone 5.8. The
   conversation is the longest lead left.

---

## Sources outside this repository

Seen on 3 October 2026 unless dated otherwise.

- **Printers.** Epson TM-T20III:
  [Technical Reference Guide](https://files.support.epson.com/pdf/pos/bulk/tm-t20iii_trg_en_reva.pdf)
  (576 and 420 dots, the one-way 58 mm conversion, bidirectional socket status, the 24 V drawer
  port) · [Jordan Computers](https://jordancomputers.com/en/tm-t20iii-012-en) (192.67 JOD) ·
  [Epson US](https://epson.com/For-Work/Printers/POS/TM-T20III-Thermal-Receipt-Printer/p/C31CH51A9972).
  Epson TM-T20IV: [launch](https://news.epson.com/news/tm-t20iv-thermal-pos-printer).
  Epson `GS v 0`, obsolete, with its per-model row limit:
  [ESC/POS reference](https://download4.epson.biz/sec_pubs/pos/reference_en/escpos/gs_lv_0.html).
  Epson TM-m10 at 420 dots:
  [specifications](https://download4.epson.biz/sec_pubs/bs/html/m000920/en/chap10_1.html) ·
  [product guide](https://files.support.epson.com/docid/cpd5/cpd50291.pdf).
  Epson TM-T88VII (180 dpi, vendor-class USB):
  [Technical Reference Guide](https://files.support.epson.com/pdf/pos/bulk/tm-t88vii_trg_en_reva.pdf).
  Bixolon SRP-332III: [user manual](https://bixolon.com/_upload/manual/Manual_User_SRP-33xIII_ENG_V1.00.pdf)
  (72 and 48 mm at 203 dpi, the one-way partition) ·
  [specification sheet](https://bixoloneu.com/wp-content/uploads/2025/04/SS_SRP-330III_EN_JAN25_V1.pdf) ·
  [TouchWindow](https://www.touchwindow.com/p/SRP-332IIIESK.html) (US price).
  Rejected 58 mm candidates: [Equip/LevelOne 351001 manual](https://www.manua.ls/equip/351001/manual) ·
  [ARKSCAN AS58U](https://www.arkscan.com/product/shipping-label-printer/thermal-receipt-printer/as58-58mm-receipt-printer).
  Backup 58 mm: [HPRT TP585](https://www.hprt.com/Product/TP585-2-inch-Thermal-Receipt-Printer-TP585-585L.html).
  Star ESC/POS additions:
  [Star, 30 January 2026](https://starmicronics.com/blog/star-micronics-expands-esc-pos-printer-compatibility-across-thermal-and-impact-printing-solutions/).
  Xprinter: [XP-N160II](https://www.xprinter.net/product/508.html).
- **Scanners.** [Datalogic QD2590 reference guide](https://cdn.datalogic.com/Download?iddwnfile=36041) ·
  [GTS Amman](https://gts.jo/en/datalogic/datalogic-quickscan-qd2590-bk-corded-handheld-omnidirectional-area-imager-barcode-scanner-black-1d-2d-and-postal-codes)
  (85 JOD) ·
  [Zebra DS2208 reference guide](https://www.zebra.com/content/dam/support-dam/en/documentation/unrestricted/guide/product/ds2208-prg-en.pdf).
- **Drawers.** [Posiflex CR-4000-G2](https://www.posiflex.com/product-detail/cr-4000-g2-series_4100-g2-series/) ·
  [PC Stores Amman](https://pcstoresfutec.com/shop) · [APG Vasario](https://posguys.com/cash-drawer_32/APG-Vasario-Series_61/) ·
  [the dinar's coins](https://www.mfa.gov.jo/content/official-currency).
- **Registers.** [Posiflex XT-3815-G2](https://www.posiflex.com/product-detail/xt-3815-g2/) ·
  [CDW](https://www.cdw.com/product/posiflex-xt-series-xt-3815-g2-all-in-one-celeron-j6412-2-ghz-8-gb-s/8448759)
  (US price) ·
  [City Center Amman, MSI PRO AP162T](https://citycenter.jo/electronics-point-sale/msi-pro-ap162t-2024-new-12gen-intel-celeron-n100-4-cores-w-15-6-ips-full-hd-anti-glare-touch-display-black) ·
  [MSI specification](https://www.msi.com/All-in-One-PC/PRO-AP162T-ADL/Specification) ·
  [GTS Amman, GSAN GS-T3](https://gts.jo/en/gsan-gs-t3-touch-pos-system-all-in-one-quad-core) (J1900, Windows 7).
- **Windows.** [Windows 11 25H2 supported Intel processors](https://learn.microsoft.com/en-us/windows-hardware/design/minimum/supported/windows-11-25h2-supported-intel-processors) ·
  [Windows 11 IoT Enterprise supported Intel processors](https://learn.microsoft.com/en-us/windows/iot/iot-enterprise/hardware/supported/winiot_intel_processors_2024) ·
  [IoT Enterprise system requirements](https://learn.microsoft.com/en-us/windows/iot/iot-enterprise/hardware/system_requirements) ·
  [IoT Enterprise LTSC 2024 lifecycle](https://learn.microsoft.com/en-us/lifecycle/products/windows-11-iot-enterprise-ltsc-2024) ·
  [WebView2 and Windows 7/8.1](https://blogs.windows.com/msedgedev/2022/12/09/microsoft-edge-and-webview2-ending-support-for-windows-7-and-windows-8-8-1/).
- **Android terminals.** [Sunmi printer developer document](https://cdn.sunmi.com/public/generalfile/mgt-document/841c6680d673447ba9c5d9b1e1131d01.pdf).
- Self-hosted runners on public repositories: [GitHub Docs, "Adding self-hosted runners"](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/adding-self-hosted-runners)
