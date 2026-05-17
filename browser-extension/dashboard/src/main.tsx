import React from "react";
import { createRoot } from "react-dom/client";
import "./tokens.css";
import { AppRouter } from "./router";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root mount node");

createRoot(root).render(
  <React.StrictMode>
    <AppRouter />
  </React.StrictMode>,
);
