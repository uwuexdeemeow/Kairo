// jsdom, which runs the web code's tests, lays nothing out, so it has no scrolling to do.
Element.prototype.scrollIntoView = function () {};

// Nor does it know the system's colors; the tests see a dark system.
window.matchMedia = (query: string) =>
  ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList;
