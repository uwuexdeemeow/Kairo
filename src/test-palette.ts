/// <reference types="vite/client" />
import page from "../palette.html?raw";

/// The palette's markup, for tests to put on the page; its scripts don't run.
export const paletteBody = page.match(/<body[^>]*>([\s\S]*)<\/body>/)![1];
