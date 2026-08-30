/**
 * POST /api/contact - relay the footer contact form to SendGrid.
 *
 * Runs server-side on Cloudflare Pages so SENDGRID_API_KEY is never exposed to
 * the browser. Bound variables (Settings -> Environment variables; set them for
 * the Production environment, and separately for Preview if you want the form
 * working on preview deployments):
 *
 *   SENDGRID_API_KEY     secret, required
 *   ADMIN_EMAIL          plaintext, required - where messages are delivered
 *   CONTACT_FROM_EMAIL   plaintext, optional - a SendGrid-verified sender.
 *                        Falls back to ADMIN_EMAIL.
 *
 * This is deliberately not an open relay: mail is only ever sent to
 * ADMIN_EMAIL. The visitor's address is used as reply-to and nothing else.
 */

const MAX = { name: 120, email: 254, message: 4000 };

const json = (status, body) =>
  new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "Cache-Control": "no-store",
    },
  });

// Deliberately simple: reject the obviously malformed, let SendGrid judge the rest.
const looksLikeEmail = (v) =>
  typeof v === "string" &&
  v.length <= MAX.email &&
  /^[^\s@,;:<>()[\]\\]+@[^\s@.,;:<>()[\]\\]+\.[^\s@,;:<>()[\]\\]{2,}$/.test(v);

// Collapse control characters, which are what header injection relies on.
// Printable punctuation is left alone so names like "Jean-Luc" survive.
const clean = (v, limit) =>
  String(v ?? "")
    .replace(/[\x00-\x1F\x7F]+/g, " ")
    .trim()
    .slice(0, limit);

export async function onRequestPost({ request, env }) {
  const { SENDGRID_API_KEY, ADMIN_EMAIL, CONTACT_FROM_EMAIL } = env;

  if (!SENDGRID_API_KEY || !ADMIN_EMAIL) {
    // Configuration problem, not the visitor's fault - do not leak which.
    console.error("contact: missing SENDGRID_API_KEY or ADMIN_EMAIL binding");
    return json(500, { error: "The contact form is not configured right now." });
  }

  // Same-origin only. Stops the endpoint being driven from another site.
  const origin = request.headers.get("Origin");
  if (origin && new URL(request.url).origin !== origin) {
    return json(403, { error: "Cross-origin requests are not accepted." });
  }

  let body;
  try {
    body = await request.json();
  } catch {
    return json(400, { error: "Expected a JSON body." });
  }

  // Honeypot: a field hidden from humans. Anything that fills it is a bot.
  // Answer 200 so the bot cannot learn it was rejected.
  if (clean(body.company, 100)) {
    return json(200, { ok: true });
  }

  // Bots submit near-instantly; real people take a few seconds to type.
  const elapsed = Number(body.elapsed);
  if (Number.isFinite(elapsed) && elapsed >= 0 && elapsed < 2000) {
    return json(200, { ok: true });
  }

  const name = clean(body.name, MAX.name);
  const email = clean(body.email, MAX.email);
  const message = String(body.message ?? "").trim().slice(0, MAX.message);

  if (!looksLikeEmail(email)) {
    return json(400, { error: "That email address does not look right." });
  }
  if (message.length < 2) {
    return json(400, { error: "Please include a message." });
  }

  const country = request.headers.get("CF-IPCountry") || "unknown";
  const from = CONTACT_FROM_EMAIL || ADMIN_EMAIL;
  const divider = "-".repeat(56);

  const payload = {
    personalizations: [{ to: [{ email: ADMIN_EMAIL }] }],
    from: { email: from, name: "Marita contact form" },
    reply_to: { email, name: name || email },
    subject: `Marita contact: ${name || email}`,
    content: [
      {
        type: "text/plain",
        value:
          `From:    ${name || "(no name given)"}\n` +
          `Email:   ${email}\n` +
          `Country: ${country}\n` +
          `Time:    ${new Date().toISOString()}\n` +
          `\n${divider}\n\n${message}\n`,
      },
    ],
  };

  let res;
  try {
    res = await fetch("https://api.sendgrid.com/v3/mail/send", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${SENDGRID_API_KEY}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
    });
  } catch (err) {
    console.error("contact: sendgrid unreachable", err);
    return json(502, { error: "Could not reach the mail service. Please try again." });
  }

  if (res.status === 202) {
    return json(200, { ok: true });
  }

  // Log the real reason; return something a visitor can act on.
  const detail = await res.text().catch(() => "");
  console.error("contact: sendgrid rejected", res.status, detail);

  if (res.status === 401 || res.status === 403) {
    return json(500, { error: "The contact form is not configured right now." });
  }
  return json(502, { error: "The message could not be sent. Please try again." });
}

// Anything other than POST.
export async function onRequest() {
  return json(405, { error: "Use POST." });
}
