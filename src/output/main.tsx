import React from "react";
import ReactDOM from "react-dom/client";

import "../styles/globals.css";
import { OutputView } from "./OutputView";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <OutputView />
  </React.StrictMode>,
);
