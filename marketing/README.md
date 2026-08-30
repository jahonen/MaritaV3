# Marketing

Static pages, deployed separately from the engine. No build step: what is in
the directory is what goes live.

```
marketing/
└── prebeta/
    ├── index.html   # Pre-Beta signup page (standalone, self-contained)
    └── _headers     # Cloudflare Pages response headers
```

## Pre-Beta page

`prebeta/index.html` is a single self-contained file. The only external
requests it makes are to Google Fonts; everything else — styles, the sensor
scope canvas, all copy — is inline. Open it directly in a browser to preview.

### Wiring up checkout

The CTA buttons read one constant near the bottom of `index.html`:

```js
var CHECKOUT_URL = ""; // e.g. "https://buy.stripe.com/xxxxxxxxxxxx"
```

While it is empty every button renders inert and relabels itself "Opening
soon", so the page is safe to publish before payments are live. Paste a Stripe
Payment Link URL in and all four CTAs activate at once.

## Deployment

Cloudflare Pages, connected to this repository. See `docs/deploy-marketing.md`.
