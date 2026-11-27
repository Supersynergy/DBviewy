# DBviewy for macOS

The packaged app runs on Apple Silicon (arm64).

## Install

1. Download **DBviewy macOS arm64** and **SHA256SUMS** from the
   [DBviewy releases](https://github.com/Supersynergy/DBviewy/releases).
2. Save both files in the same folder and run `shasum -a 256 -c SHA256SUMS`
   in that folder. Do not install if verification fails.
3. Extract the ZIP. Quit any running copy of DBviewy. Keep the old app
   as a backup before copying `DBviewy.app` into `/Applications` or
   `~/Applications`. You do not need to change or delete your settings.

The app is ad hoc signed, not notarised. If macOS blocks it after you have
verified the checksum, follow the per-app **Open Anyway** option under
**System Settings → Privacy & Security**. Leave Gatekeeper enabled for all
other apps. The Octa installer does **not** install DBviewy.

On a Mac, **Check for Updates** links to the DBviewy releases. Download
the new bundle there and verify its checksum; DBviewy does not install
an update over the running app.

## Run from the command line

```bash
/Applications/DBviewy.app/Contents/MacOS/octa [file]
```

Optionally link it onto your `PATH` (`/usr/local/bin` does not exist on a
fresh Apple Silicon Mac):

```bash
mkdir -p ~/.local/bin
ln -s /Applications/DBviewy.app/Contents/MacOS/octa ~/.local/bin/octa
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zprofile
```

## Remove or roll back

Quit DBviewy and move the application bundle to the Trash, or replace it
with the previous app. If you created a command-line link, remove that
link separately. This does not delete documents or settings.

In Finder, **Open With → DBviewy** opens an Excel workbook or data file.
DBviewy registers as an alternate editor. Installing it does not change
your default apps.
