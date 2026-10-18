# Building the Windows VST3 through GitHub Actions (no local compiling)

This lets GitHub's own Windows machines build the plugin for you. Nothing runs on your
computer, so Smart App Control never gets a say.

## 1. Get the project into a GitHub repo

If you don't already have a repo for this, make one: on github.com, click **New repository**,
give it any name, create it.

Then add this `tape-machine-vst3` folder (the one you got from me, now including a
`.github/workflows/build-windows.yml` file inside it) to that repo. Easiest way, entirely in
the browser, no git needed:

1. Open your repo on github.com.
2. Click **Add file → Upload files**.
3. Drag the whole `tape-machine-vst3` folder (with its `.github` subfolder intact) into the
   upload area. GitHub preserves the folder structure.
4. Scroll down, click **Commit changes**.

(If you're comfortable with git and it already works on your machine, `git add` / `git commit`
/ `git push` works too — git itself wasn't what got blocked, `rustc.exe` was.)

## 2. Let it build

Pushing the files via step 1 automatically starts the build (the workflow triggers on push).
To watch it or re-run it later:

1. Go to your repo's **Actions** tab.
2. You'll see a run called **"Build VST3 (Windows)"**, either already running or queued.
3. Click into it. First run takes roughly 5–10 minutes (installing Rust and compiling
   everything from scratch on GitHub's machine); it also re-runs the same parity tests I used
   locally, so you'll see those pass or fail right there too.
4. If you ever want to re-trigger it without changing anything, use the **"Run workflow"**
   button on the workflow's page in the Actions tab.

## 3. Download the result

1. Once the run finishes (green check), click into it.
2. Scroll to the **Artifacts** section at the bottom of the run summary page.
3. Download **tape-machine-vst3-windows** — it's a zip containing the real
   `tape-machine-vst3.vst3` folder and `.clap` file, built on an actual Windows machine.
4. Unzip it, then follow step 3 ("Install it") from `BUILD_WINDOWS.md` — copy the `.vst3`
   folder into `C:\Program Files\Common Files\VST3\` (the `install.ps1` script works here too,
   just point it at wherever you unzipped the artifact instead of `target\bundled`).

## Notes

- Public repos get GitHub Actions minutes for free. If your repo is private, free accounts get
  a limited monthly quota (plenty for occasional rebuilds of a small project like this).
- The workflow only triggers automatically on changes inside `tape-machine-vst3/` or the
  workflow file itself, so it won't fire on unrelated commits to the rest of your repo.
- This builds and uploads an **unsigned** plugin, same as a local build would. If your DAW or
  Windows itself challenges the DLL when your actual tape machine app (the DAW) loads it,
  that's a separate, unresolved question from the compiler-blocking issue — worth testing once
  you have the file.
