import { mount } from "svelte";
import App from "./App.svelte";
import { i18n } from "./lib/i18n.svelte";
import "./app.css";

// `<html lang>` and the shared components' labels before the first paint.
i18n.apply();

mount(App, { target: document.getElementById("app")! });
