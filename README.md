# leogit

A calm Git app for Mac, Windows and Linux. It's for people who already know Git and want the everyday stuff to be quick.

## What you can do

- See your changes, pick what goes in, and commit
- Let AI write the commit message, using Claude or your own Ollama
- Browse history, then cherry-pick, squash or reorder commits, with an undo if you change your mind
- Switch, create, merge and delete branches
- Push, pull and publish to GitHub from one button that figures out what's needed next
- Use a terminal built right into the window

It runs real Git under the hood, so you can jump between the app and your own terminal whenever you like.

## Install

On Mac or Linux, paste this in a terminal:

```bash
curl -fsSL https://raw.githubusercontent.com/LeoManrique/leogit/main/scripts/install.sh | bash
```

Run it again whenever you want to upgrade. It also adds a `leogit` command, so you can open a project straight from your terminal. On a Mac account without admin rights, macOS asks for an administrator's name and password.

On Windows, or if you'd rather install by hand, grab a download from the [latest release](https://github.com/LeoManrique/leogit/releases/latest).

## What you need

- macOS 26+, Ubuntu 20.04+, or Windows 10+
- Git 2.45 or newer
- FUSE 2 on Linux, which some distros don't include (on Arch: `sudo pacman -S fuse2`)
- Optional: the [GitHub CLI](https://cli.github.com), to clone your repos and publish to GitHub from the app

## Learn more

- [DESIGN.md](DESIGN.md): what the app does
- [STYLE.md](STYLE.md): how it looks
- [TECHNICAL.md](TECHNICAL.md): how it's built, and how to build it yourself
- [FRONTEND.md](FRONTEND.md): what the Mac and Windows/Linux apps have to agree on
- [ROADMAP.md](ROADMAP.md): what's next

## License

MIT
