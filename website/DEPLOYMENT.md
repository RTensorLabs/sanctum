# Deploy the existing Sanctum website

This is the existing `website/` folder from the Sanctum repository, originally
published with GitHub Pages. Its HTML, CSS and local assets have been redesigned;
it is not a separate website repository or a hosted app backend.

Fonts and example app screenshots are local. The page has no analytics, trackers,
third-party font requests or private credentials. Screenshots contain sample profiles
and conversations. APKs and source code remain on GitHub; website hosting does not
change Sanctum's message or update routes.

## Motion

`particles.js` draws the interactive constellation field. `motion.js` coordinates
scroll choreography without a visitor-facing animation control. Particle drawing stops offscreen
and while the browser tab is hidden. Reduced-motion and data-saving preferences
keep the effects still. App screenshots and the share preview are static local images.

## Current Vercel project

- Workspace: `tejas-gowda-ss-projects`
- Project: `sanctum-rtensorlabs`
- Project ID: `prj_7anjk2INml08YMF9TbE2C8n0VQzi`
- Production URL: https://sanctum-rtensorlabs.vercel.app/
- Framework: Other; root: `website`; no build or install command

The project was created from the existing folder. The first Git connection attempt
reported that Vercel's GitHub integration is not installed for the repository.
Source-file deployment through the authenticated Vercel API does not need that
connection. GitHub push-to-Vercel publishing stays pending until the integration
is installed and the repository connection is verified. The Pages workflow still
publishes every website change in the meantime.

## Vercel

Use the intended Vercel account and deploy this existing folder. If the account
allows a Git connection to the public repository, import Sanctum
(`RTensorLabs/sanctum`) with these settings:

- Root directory: `website`
- Framework preset: Other
- Build command: leave empty (the configuration disables builds)
- Output directory: `.`
- Install command: none
- Environment variables: none

Vercel supplies a project address ending in `.vercel.app`, with the exact name depending
on availability. This free hosting subdomain needs no domain purchase. A registered
custom domain can be connected later. Do not purchase a domain or upgrade a plan as
part of this setup. Vercel's Hobby plan is intended for personal, non-commercial
projects; review its terms if the project's use changes.

Check the selected account's Git connection limits before relying on automatic
deployments from the organization. Vercel's limits documentation restricts Hobby
connections to organization-owned repositories, while its Git guide explicitly
describes the restriction for private organization repositories. Verify the public
repository flow in the connected account; do not accept a paid upgrade or trial.
If the Git connection is unavailable, the authenticated CLI can deploy this same
`website/` directory without importing a separate repository (`vercel --prod` from
this directory). Automatic publishing then needs a separately configured supported
deployment flow; it is not provided by `vercel.json` alone.

The `vercel.json` file supplies security headers and a short `/download` redirect.
Keep analytics and Speed Insights disabled. Check deployment protection so the
production website is public. Set the repository homepage to the verified production
URL after deployment. Use that URL for canonical metadata once it is known.

## Existing GitHub Pages

The existing Pages workflow still publishes `website/` and can remain as a fallback
until the Vercel deployment is verified. Website-only changes do not build an APK.
All local asset links are relative, so the site also works under a project subpath.

Moving a repository does not redirect its old GitHub Pages URL. After the repository
moves, the Pages project URL is `https://rtensorlabs.github.io/sanctum/`. Verify the
Pages workflow and announce the final Vercel URL once it exists.

## Repository transfer and installed apps

Current builds look for updates at `RTensorLabs/sanctum` first and try the old
`Tejascodex001/sanctum` address only if that fails (see `REPOS` in
`app/rust/src/api/update.rs`). Builds from before that change only know the old
address and reach the new one through GitHub's repository redirect; builds older than
the redirect-handling update may need a one-time manual APK upgrade after the move.
Do not recreate `Tejascodex001/sanctum` at the old location: doing so removes GitHub's
repository redirect, which those older installed apps still depend on. Android only
installs updates signed with the app's own key, so a repository at the old address
can't push a different app, but it could stop older installs from updating.

The repository transfer preserves history, releases and repository secrets. Verify
the destination is public and that the transferred APK assets remain downloadable.

## Local preview

Run `python3 -m http.server 4173 --directory website` from the repository root.
Check 320px, 393px, tablet and desktop widths, light/dark themes, keyboard navigation,
the mobile menu, reduced motion, and the download/source links.

References:
- https://vercel.com/docs/domains/working-with-domains/add-a-domain
- https://vercel.com/docs/plans/hobby
- https://vercel.com/docs/limits#connecting-a-project-to-a-git-repository
- https://vercel.com/docs/git#using-hobby-teams
- https://vercel.com/docs/cli/deploy
- https://docs.github.com/en/repositories/creating-and-managing-repositories/transferring-a-repository

The transfer to `RTensorLabs/sanctum` is complete. Website download and source
links use that canonical organization URL. The app retains its legacy repository
entry point for installed-client compatibility; Build 83 follows the ownership
redirect and then the release redirect. The configured feedback receiver is a
separate Tor service on the developer's phone and is independent of repository
ownership or website hosting.
