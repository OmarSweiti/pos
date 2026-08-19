import { create } from "zustand";

interface CartState {
  totalMinor: number;
  parts: number;
  splits: number[];
  setTotalMinor: (v: number) => void;
  setParts: (v: number) => void;
  setSplits: (v: number[]) => void;
}

export const useCart = create<CartState>((set) => ({
  totalMinor: 1000,
  parts: 3,
  splits: [],
  setTotalMinor: (totalMinor) => set({ totalMinor }),
  setParts: (parts) => set({ parts }),
  setSplits: (splits) => set({ splits }),
}));
