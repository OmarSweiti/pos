import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { installLocale } from "./i18n";
import "./index.css";
import "./styles/font.css";

const queryClient = new QueryClient();

// Before the first paint, and before anything reads the root. `index.html`
// ships `<html lang="ar" dir="rtl">` too, but a served attribute is not the
// product deciding: this is the line that makes Arabic the rendered default
// whatever document the register is booted from (microstep 1.11.1).
installLocale(document.documentElement);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
