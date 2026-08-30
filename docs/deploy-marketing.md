# Deploying the marketing site

The marketing pages are static. There is no build step: Cloudflare Pages serves
`marketing/prebeta/` exactly as committed.

## One-time setup (Cloudflare dashboard)

You have to do this part yourself — it needs your Cloudflare and GitHub
credentials.

1. Cloudflare dashboard → **Workers & Pages** → **Create** → **Pages** →
   **Connect to Git**.
2. Authorise the Cloudflare GitHub app and pick `jahonen/MaritaV3`. The repo is
   private; grant access to just this repository rather than all of them.
3. Configure the build:

   | Setting | Value |
   | --- | --- |
   | Production branch | `main` |
   | Framework preset | None |
   | Build command | *(leave empty)* |
   | Build output directory | `marketing/prebeta` |
   | Root directory | *(leave empty)* |

4. **Save and Deploy.**

That is the whole integration. Every push to `main` redeploys; pushes to other
branches get their own preview URL. Because there is no build command, deploys
take a few seconds even though the repository is mostly Rust.

## Updating the page

Edit `marketing/prebeta/index.html`, commit, push to `main`. Cloudflare picks it
up automatically.

```bash
git add marketing/prebeta/index.html
git commit -m "Update pre-Beta page"
git push
```

Rollback is in the dashboard: **Deployments** → pick an earlier one →
**Rollback**. No revert commit needed.

## Custom domain

Pages → your project → **Custom domains** → **Set up a domain**. If the domain's
nameservers are already on Cloudflare the DNS record is created for you; if not,
you will be given a CNAME to add at your registrar. TLS is provisioned
automatically.

## Alternative: deploy from GitHub Actions

Only needed if you want deploys to fire *only* when `marketing/**` changes, or
want to run checks before publishing. `.github/workflows/deploy-marketing.yml.example`
has a working workflow — rename it to drop `.example`, then add two repository
secrets under **Settings → Secrets and variables → Actions**:

- `CLOUDFLARE_API_TOKEN` — a token with the **Cloudflare Pages: Edit** permission.
- `CLOUDFLARE_ACCOUNT_ID` — from the Cloudflare dashboard URL or the Workers &
  Pages overview.

Do not enable both paths at once. If the Git integration is already connected,
pick one and disconnect the other, or every push deploys twice.

## Response headers

`marketing/prebeta/_headers` sets security headers and keeps `index.html`
revalidating so an update is visible immediately rather than sitting in a CDN
cache. Cloudflare Pages reads this file from the output directory; it is not
served to visitors.

## IndexNow

IndexNow is a push protocol: rather than waiting to be crawled, you tell search
engines that a URL changed. **Bing, Yandex, Seznam, Naver and Yep participate.
Google does not** — it ignores IndexNow entirely, so Search Console and ordinary
crawling remain the only route there.

There are two ways to do this and they can coexist.

### Option A — Cloudflare Crawler Hints (recommended, zero maintenance)

Cloudflare is an IndexNow partner and can submit changes for you automatically,
with no key file and no script.

Cloudflare dashboard → your domain → **Caching** → **Configuration** →
enable **Crawler Hints**.

That is the whole setup. Cloudflare watches for content changes and notifies
IndexNow participants on your behalf.

### Option B — explicit key and submission script

Useful when you want to submit at a precise moment, for example immediately
after a deploy, or to submit specific URLs.

The key is already in place:

- Key file: `marketing/prebeta/<key>.txt`, served at the domain root. It is a
  self-chosen random string, published deliberately — it proves you control the
  domain and is not a secret.
- Submitter: `scripts/indexnow.sh`

```bash
scripts/indexnow.sh                     # submit the home page
scripts/indexnow.sh https://marita-universe.com/
```

The script refuses to submit if the key file is not reachable, since that is the
usual cause of an otherwise silent rejection.

Response codes: `200`/`202` accepted, `403` key not valid for the host, `422`
URL/key mismatch, `429` rate limited.

### When to submit

Submit when page *content* meaningfully changes — new copy, new sections, a
price change. Do not submit on every deploy; repeatedly pushing an unchanged URL
is what gets a host rate limited. There is no benefit to submitting more than
once for the same change.
