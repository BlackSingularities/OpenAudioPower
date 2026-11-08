import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

if (import.meta.env.DEV) {
  const { installDevMock } = await import("./devmock");
  installDevMock();
}

const app = mount(App, { target: document.getElementById("app")! });
export default app;
