import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App.tsx";

import "./style.css";

const root = document.querySelector("#root");

if (!root) {
  throw new Error("popup 的 #root 不存在,index.html 可能被改壞了");
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
