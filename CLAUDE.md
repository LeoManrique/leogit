# LeoGit Instructions for Claude Code

## Implementation Log

Constantly update these documents without repeating information between them.

- Update `./STYLE.md` after significantly changing the frontend style. Remove obsolete information.
- Update `./DESIGN.md` after completing any functional design feature or big change. Remove obsolete information.
- Update `./TECHNICAL.md` after changing any technical architecture or implementation detail. Remove obsolete information.
- Update `./ROADMAP.md` after completing any point mentioned there. Also add new features that come up while developing.
- Update `./README.md` after completing some change that affects the escense of the project. Keep it concise and remove obsolete information.

## Implementation order

Don't implement too multiple features at once, make the feature implementation order what the user flow is, and make sure it is tested before proceeding to the next one.

## Long-term correctness over immediate fixes

- Our objectives are now not about fixing bugs, instead we need to focus on making the app as solid as possible. When deciding something try to take the approach that involves a long-term correctness.

## Clean, Mantainable, and Readable Code

Follow DRY and Single Responsability principles to make our code clean, mantainable, and readable code. Also give files and folders a meaningful name.

Run clippy pedantic to see if rust recommendations are being followed.

## Always fix

Do not ignore the errors or warnings when building or running the projects, even if they were already present or not caused by your changes. Do not bypass errors or warnings, actually fix them.

## Debug mode

For key features in both the frontend and backend, keep meaninful logs that will be useful in case of errors. Dont delete the most important ones until we make it to production, the platform is still under development.

## Prefer newer versions

If we have to decide between downgrading or updating versions of packages or libraries because of an issue, always choose upgrading. Never downgrade them to fix an issue.
