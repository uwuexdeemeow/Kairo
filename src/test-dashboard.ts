/// <reference types="vite/client" />
import page from "../dashboard.html?raw";

/// The dashboard's markup, for tests to put on the page; its scripts don't run.
export const dashboardBody = page.match(/<body>([\s\S]*)<\/body>/)![1];
