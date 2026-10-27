# Build and publish a Branchroom release

Use **Method A (Git commands)** or **Method B (GitHub Desktop)**. You do not need to do both.

## What happens automatically?

| What you send to GitHub | What happens |
| --- | --- |
| A normal commit pushed to a branch | GitHub checks the code and builds Windows, Linux, and macOS downloads. |
| A version tag starting with `v`, pushed to GitHub | GitHub runs the checks and builds, then publishes a release if every build succeeds. |
| A manual run from the Actions page | GitHub checks and builds the code. This does not publish a release. |

A **commit** saves your code. A **tag** gives one commit a version label. A **release** is the download page GitHub creates for that version.

For example:

- `v0.2.0-rc.2`: a test release, marked **Pre-release** because the name contains a hyphen.
- `v0.2.0`: a regular release.

The examples below use `0.2.0-rc.2`. If that version already exists, choose a new version and replace it everywhere below.

## Before either method

1. Open the Branchroom source repository connected to your GitHub repository.
2. Select the branch containing the code you want to release. If your project requires pull requests, merge your changes into the release branch first.
3. Make sure `.github/workflows/build.yml` is included in the changes you will publish. GitHub Actions must be enabled for the repository.
4. Open `Cargo.toml`. Under `[package]`, change the version:

   ```toml
   version = "0.2.0-rc.2"
   ```

5. Open a terminal in the repository folder and run:

   ```powershell
   cargo check
   ```

   This checks the code and updates `Cargo.lock` to match the new package version. Include both files in your release commit. Rust and the project's native build dependencies must be installed; see [Build and verify](../README.md#build-and-verify).

The tag should match the package version, with an extra `v` at the start: package `0.2.0-rc.2`, tag `v0.2.0-rc.2`.

## Method A: Git commands

Run these commands from the repository folder.

### 1. Review and save the release changes

```powershell
git status
git diff
```

Review the files before staging them, including any new files listed by `git status`. Then save the intended changes:

```powershell
git add .
git commit -m "Prepare v0.2.0-rc.2"
```

If the changes are already committed, skip that commit command.

### 2. Send the code to GitHub

```powershell
git push origin HEAD
```

This sends your current branch. A normal build starts on GitHub. You can check it under **Actions > Build and test**.

### 3. Mark this commit as the release version

```powershell
git tag -a v0.2.0-rc.2 -m "Branchroom v0.2.0-rc.2"
git push origin v0.2.0-rc.2
```

The first command creates the version label on your computer. The second sends it to GitHub and starts the release workflow. A normal branch push alone does not publish this tag.

Continue with **Check the release** below.

## Method B: GitHub Desktop

GitHub Desktop handles committing and publishing the tag. You still need the `cargo check` command from **Before either method** to update the Rust lockfile.

### 1. Save the release changes

1. Open **GitHub Desktop**.
2. Choose Branchroom under **Current repository**.
3. Check **Current branch** is the branch you want to release.
4. Open the **Changes** tab and review the files selected for the commit. Include the application changes, workflow, `Cargo.toml`, and `Cargo.lock`.
5. Enter `Prepare v0.2.0-rc.2` in the **Summary** field.
6. Click **Commit to [your branch]**.

If the release changes are already committed, go directly to the next step.

### 2. Create the version label

1. Open the **History** tab.
2. Right-click the release commit you just created.
3. Choose **Create Tag...**.
4. Enter `v0.2.0-rc.2`.
5. Click **Create Tag**.

Make sure you select the commit containing all the release changes. The tag marks that exact code.

### 3. Send it to GitHub

1. Click **Push origin** in the top bar. If this is a new branch, publish the branch when prompted.
2. GitHub Desktop pushes your new tag with the commit by default.
3. In **History**, select the tagged commit. An upward arrow beside the tag means it has not been pushed yet.
4. Open your repository on GitHub and check that the tag appears under **Tags**.

If the tag is still local after pushing, open a terminal in the repository and run:

```powershell
git push origin v0.2.0-rc.2
```

GitHub's instructions: [Managing tags in GitHub Desktop](https://docs.github.com/en/desktop/managing-commits/managing-tags-in-github-desktop) and [Pushing changes](https://docs.github.com/en/desktop/making-changes-in-a-branch/pushing-changes-to-github-from-github-desktop).

## Check the release

1. Open your repository on **GitHub in a browser**.
2. Click **Actions**, then **Build and test**.
3. Open the run for your version tag, such as `v0.2.0-rc.2`. A branch run may also appear; the tag run publishes the release.
4. Wait for the Windows, Linux, and macOS build jobs to turn green.
5. Wait for **Publish Release** to turn green.
6. Return to the repository's main page and open **Releases**.
7. Open your version and expand **Assets**.

You do not need to click **Draft a new release**. The workflow creates the release and generates release notes automatically.

## Which download should I use?

| System | Download |
| --- | --- |
| Windows | The Branchroom `.zip` package. Extract it, then run `branchroom.exe` inside. A standalone `.exe` is also provided. |
| Linux | The `.tar.gz` asset with `Linux` in its name. Extract it to get the executable. |
| macOS | The `.tar.gz` asset with `macOS` in its name. Extract it to get the executable. |

Linux and macOS archives are built for the architecture of their GitHub runner; they are not universal packages. These builds are unsigned and do not include an installer. On Windows, Git must be installed separately. GitHub CLI is needed for GitHub features.

The automatically generated **Source code** downloads contain source files, not a ready-to-run application. `latest.json` is build metadata; it is not an installer.

### Download a build without publishing a release

1. Push your changes normally, without creating a version tag.
2. Open **Actions > Build and test** and select the successful run.
3. Scroll to **Artifacts**.
4. Download `branchroom-Windows`, `branchroom-Linux`, or `branchroom-macOS`.
5. Extract the downloaded artifact. For Windows, extract the packaged ZIP inside it to use the complete package.

You may need to sign in to GitHub to download workflow artifacts.

## If something goes wrong

| Problem | What to do |
| --- | --- |
| No release appears after a normal push | Push a new version tag beginning with `v`. A branch push only runs builds. |
| The tag exists locally but no release run starts | Push the tag to GitHub. Also check Actions is enabled and the tagged commit includes the workflow file. |
| `tag already exists` | Use a new version number. Update the package version and lockfile, commit them, then create the new matching tag. |
| A check or build fails | Open the red job in Actions and read the failed step. A failed build prevents publication. |
| You fixed code after a failed release | Commit the fix and publish a new version tag on the corrected commit. The old tag still points to the old code. |
| A temporary runner or network failure occurs | Use **Re-run failed jobs** in Actions. No source change or new tag is needed for a temporary failure. |
| Release creation reports a permission error | Check repository or organization Actions restrictions. The workflow requests `contents: write` for the release job. |
| `cargo check` fails locally | Fix the reported build error before committing the release. Follow the README's Rust and native dependency setup. |

For the next release, repeat the same steps with a new version number.
