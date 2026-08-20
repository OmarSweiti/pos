import { create } from "zustand";
import {
  applyLocale,
  DEFAULT_LOCALE,
  type Locale,
  toggled,
} from "../lib/direction";

interface LocaleState {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  /** Switch to the opposite of the current selection. */
  toggle: () => void;
}

/**
 * The document root is the single place direction lives, so any component can
 * read it from CSS instead of threading a prop. index.html ships the same
 * default, so there is no flash of the wrong direction before this store boots.
 */
export const useLocale = create<LocaleState>((set, get) => ({
  locale: DEFAULT_LOCALE,
  setLocale: (locale) => {
    applyLocale(document.documentElement, locale);
    set({ locale });
  },
  toggle: () => {
    get().setLocale(toggled(get().locale));
  },
}));
