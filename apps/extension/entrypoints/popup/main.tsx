import React from "react";
import ReactDOM from "react-dom/client";

import App from "./app.tsx";

import "./style.css";

const root = document.querySelector("#root");

if (!root) {
  throw new Error("the popup has no #root; index.html may have been broken");
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
