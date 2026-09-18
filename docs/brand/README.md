# DarkHorse identity

The **Split Mane** direction uses an angular horse profile with a long sloping face, a defined muzzle and jaw, and an open mane channel. A forward-leaning head, stronger neck, swept-back ears, and a narrow angled eye give the mark an assertive expression. Angular, custom-drawn lettering echoes the symbol while the restrained mint and off-white palette matches the console.

Open [the design board](preview.html) to review the logo, dark-surface treatments, actual pixel sizes, and console-header treatment. The approved logo is applied to the sign-in and authorization headers, browser favicon, and project README. This is a visual-identity contribution to the console work in issue #15; it does not complete that issue.

## Assets

| Asset                                                                 | Use                                                                                             |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| [Full signature](darkhorse-identity-server.svg)                       | Symbol, DARKHORSE lettering, and IDENTITY SERVER descriptor; presentations and large placements |
| [README banner](readme-banner.svg)                                    | Full signature on a near-black background for readability in light and dark document themes     |
| [Compact signature](../../apps/console/src/lib/assets/brand/logo.svg) | Product navigation; no descriptor                                                               |
| [Mint symbol](../../apps/console/src/lib/assets/brand/mark.svg)       | App marks and square placements                                                                 |
| [Monochrome symbol](darkhorse-mono.svg)                               | One-color reproduction on dark surfaces                                                         |
| [Favicon](../../apps/console/src/lib/assets/favicon.svg)              | Small browser icon with a dark-green backing tile                                               |

All SVG artwork uses paths, contains no external resources or font dependencies, and has a transparent background except for the favicon and README banner. The full signature's small descriptor is also vector geometry. The mint and monochrome symbols have identical silhouettes.

## Use

- Primary symbol: mint `#91e4b4`; lettering: off-white `#eaf0ed`; descriptor: muted green `#9aaea2`.
- Preferred backgrounds: near-black `#090d0c`, deep green `#102018`, and the existing console glass surfaces.
- Use the compact signature at **200 px wide or larger**; the console uses **224 px**. Use the full signature at **420 px wide or larger** to keep the descriptor legible.
- Use the bare symbol at **24 px or larger**. Use the backing-tile favicon for **16 px** browser placements.
- Keep at least **8 units on the 64-unit symbol grid** of clear space around the artwork. Retain the SVG viewBox padding and original proportions.
- Keep the mark flat and opaque. Avoid stretching, hairline outlines, shadows, and glow effects.
- Give a standalone image an appropriate alternative such as `DarkHorse Identity Server`. For a home link, use a useful link name such as `Darkhorse home`.

The artwork is authored for this project. No third-party icon or font is embedded.
