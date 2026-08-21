import { formatMinor, JOD } from "@pos/money";
import { invoke } from "@tauri-apps/api/core";
import { directionFor, toggleLabel } from "./lib/direction";
import { useCart } from "./store/cart";
import { useLocale } from "./store/locale";

export default function App() {
  const { totalMinor, parts, splits, setTotalMinor, setParts, setSplits } =
    useCart();
  const { locale, toggle } = useLocale();

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
            value={totalMinor}
            onChange={(e) => setTotalMinor(Number(e.target.value))}
            className="w-40 rounded bg-zinc-800 px-3 py-3 text-lg"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          Split into
          <input
            type="number"
            min={1}
            value={parts}
            onChange={(e) => setParts(Number(e.target.value))}
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
