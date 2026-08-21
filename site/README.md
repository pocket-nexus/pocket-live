# Pocket Live site

The product site for [Pocket Live](https://github.com/dozycat/pocket-live), a
fully local macOS camera-to-VRM motion-capture pipeline.

## Local development

Requires Node.js `>=22.13.0`.

```bash
npm install
npm run dev
```

## Cloudflare Worker deployment

The production Worker is `pocket-live` in account
`5e97b4b91f8abaf2de54de9f866bfcae`, with the custom domain
`live.pocketlab.build`.

The existing `pocketlab` Worker and the apex/`www` hostnames are separate and
must not be modified by this deployment.

```bash
npx wrangler login
npm run deploy:cloudflare
```

## Validation

```bash
npm run build
npx tsc --noEmit
npx eslint app tests --max-warnings=0
node --test tests/rendered-html.test.mjs
```

The demo in `public/media/` is provided as WebM and MP4. Both are muted,
18-second cuts below 1 MB. Product claims come from the Pocket Live branch and
its local camera, tracking, plugin, and OBS documentation.
