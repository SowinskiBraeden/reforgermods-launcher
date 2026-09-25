import { render } from "preact";
import { App } from "./app";
import { start } from "./lib/state";
import "./styles.css";

const root = document.getElementById("app");
if (!root) throw new Error("missing #app root element");

render(<App />, root);

// Kick off data loading after the first paint, so the chrome is on screen
// immediately rather than waiting on the network.
void start();
