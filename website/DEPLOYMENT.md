# Deploy the existing Sanctum website

This is the existing `website/` folder from the Sanctum repository, originally
published with GitHub Pages. Its HTML, CSS and local assets have been redesigned;
it is not a separate website repository or a hosted app backend.

Fonts and example app screenshots are local. The page has no analytics, trackers,
third-party font requests or private credentials. Screenshots contain sample profiles
and conversations. APKs and source code remain on GitHub; website hosting does not
change Sanctum's message or update routes.

## Vercel

Use the intended Vercel account and deploy this existing folder. If the account
allows a Git connection to the public repository, import Sanctum
(`RTensorLabs/sanctum` after its transfer) with these settings:

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

Publish the repository-redirect compatibility app update before changing owners.
Builds older than that update may need a one-time manual APK upgrade after the move.
Do not recreate `Tejascodex001/sanctum` at the old location: doing so removes GitHub's
repository redirect, which installed clients use as a compatibility entry point.

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

During migration, download and source links may temporarily use the legacy GitHub
address so they work before and after the transfer. Update them to the canonical
organization URL after the destination repository has been verified.
