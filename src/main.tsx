import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./views/App";
import Hud from "./views/Hud";
import Bubble from "./views/Bubble";
import { brand } from "./lib/api";
import "./styles.css";

document.documentElement.style.setProperty("--accent", brand.accent);
const route = window.location.hash;
const View = route.startsWith("#/hud") ? Hud : route.startsWith("#/bubble") ? Bubble : App;
document.body.classList.add(route.startsWith("#/hud") ? "is-hud" : route.startsWith("#/bubble") ? "is-bubble" : "is-main");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <View />
  </StrictMode>,
);
