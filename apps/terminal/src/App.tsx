import { formatMinor, JOD } from "@pos/money";
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { directionFor, toggleLabel } from "./lib/direction";
import { parseCountInput, parseMinorInput } from "./lib/minorInput";
import { useCart } from "./store/cart";
import { useLocale } from "./store/locale";

export default function App() {
  const { totalMinor, parts, splits, setTotalMinor, setParts, setSplits } =
    useCart();
  const { locale, toggle } = useLocale();

  // The draft is what the person typed; the store only ever holds a value the
  // money guard accepted. Keeping both is what lets a half-typed "1." stay on
  // screen without ever reaching an i64.
  const [totalDraft, setTotalDraft] = useState(String(totalMinor));
  const [partsDraft, setPartsDraft] = useState(String(parts));
  const [refusal, setRefusal] = useState<string | null>(null);

  function onTotalChange(raw: string) {
    setTotalDraft(raw);
    const parsed = parseMinorInput(raw);
    if (parsed.ok) {
      setTotalMinor(parsed.value);
      setRefusal(null);
    } else {
      setRefusal(`total: ${parsed.reason}`);
    }
  }

  function onPartsChange(raw: string) {
    setPartsDraft(raw);
    const parsed = parseCountInput(raw);
    if (parsed.ok) {
      setParts(parsed.value);
      setRefusal(null);
    } else {
      setRefusal(`split into: ${parsed.reason}`);
    }
  }

  async function split() {
    const result = await invoke<number[]>("split_tender", {
      totalMinor,
      parts,
    });
    setSplits(result);
  }

  return (
    <main className="min-h-screen bg-zinc-950 p-8 text-zinc-100">
      <header className="mb-6 flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold">
          POS Terminal — Phase 0 smoke panel
        </h1>
        <button
          type="button"
          onClick={toggle}
          lang={locale === "ar" ? "en" : "ar"}
          className="min-h-12 rounded border border-zinc-700 px-4 py-3 text-lg active:bg-zinc-800"
        >
          {toggleLabel(locale)}
        </button>
      </header>

      <p className="mb-6 font-mono text-sm text-zinc-400">
        lang={locale} dir={directionFor(locale)}
      </p>

      <div className="flex flex-wrap items-end gap-4">
        <label className="flex flex-col gap-1 text-sm">
          Total (minor units)
          <input
            type="number"
            value={totalDraft}
            onChange={(e) => onTotalChange(e.target.value)}
            className="w-40 rounded bg-zinc-800 px-3 py-3 text-lg"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          Split into
          <input
            type="number"
            min={1}
            value={partsDraft}
            onChange={(e) => onPartsChange(e.target.value)}
            className="w-24 rounded bg-zinc-800 px-3 py-3 text-lg"
          />
        </label>
        <button
          type="button"
          onClick={split}
          className="min-h-12 rounded bg-emerald-600 px-6 py-3 text-lg font-medium active:bg-emerald-700"
        >
          Split via Rust
        </button>
      </div>

      {refusal !== null && (
        <p role="alert" className="mt-4 font-mono text-sm text-amber-400">
          refused — {refusal}. Sending {formatMinor(totalMinor, JOD)} JOD in{" "}
          {parts}.
        </p>
      )}

      {splits.length > 0 && (
        <ul className="mt-6 space-y-1 font-mono">
          {splits.map((s, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: tender position *is* its identity
            <li key={i}>
              tender {i + 1}: {formatMinor(s, JOD)}
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}
