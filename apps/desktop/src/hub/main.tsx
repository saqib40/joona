import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@/styles/globals.css";
import { isTauri } from "@/lib/utils";
import { App } from "./App";
import { connect } from "./connect";

async function start() {
  // The demo's mocks make the page look like Tauri, so ask first.
  if (!isTauri()) {
    const { installDemo } = await import("./demo");
    installDemo();
  }
  void connect();
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void start();
