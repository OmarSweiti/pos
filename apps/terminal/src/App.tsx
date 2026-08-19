import { invoke } from "@tauri-apps/api/core";
import { useCart } from "./store/cart";
import { formatMinor } from "./lib/money";

export default function App() {
  const { totalMinor, parts, splits, setTotalMinor, setParts, setSplits } = useCart();

  async function split() {
    const result = await invoke<number[]>("split_tender", { totalMinor, parts });
    setSplits(result);
  }

  return (
    <main className="min-h-screen bg-zinc-950 p-8 text-zinc-100">
      <h1 className="mb-6 text-2xl font-semibold">POS Terminal — Phase 0 smoke panel</h1>

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
            <li key={i}>
              tender {i + 1}: {formatMinor(s)}
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}
