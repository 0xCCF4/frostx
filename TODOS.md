# Todos

List of features, I would like to add, in no particular order. Not a roadmap, just a brain dump of ideas.

- [ ] Frostx should still be able to run when the project points to an archive (e.g. created by archive.compress) to run further pipelines, hence, it must inspect the archive, open the frostx.toml inside
- [ ] `git` and `jj` actions currently change the modification time of the project folder - not ideal - need to find a
    solution.
- [ ] For verification that the project is put to the remote folder, we must always check the integrity of the upload,
  if the backend does not support that natively, we need to download the uploaded file and compare checksums, or
  something like that. Might mark then the completion to only do this once. For the ssh upload, not rsync, this should
  be done via remote checksum command, if supported, download only as fallback.
- [ ] all easy registration for new urls besides rsync, ssh, via trait abstraction?
- [ ] The cleanup action should be agnostic of the files present, e.g. instead of hardcoding `target/` and
  `node_modules/`, it should check e.g. the presence of `Cargo.toml` and `package.json` and only then clean those
  folders. The user might hardcode some paths in addition to these options (which should also be enabled/disabled via
  config). Probably best here a trait such that easily new cleaners can be added, e.g. `RustCleaner`, `NodeCleaner`,
  `PythonCleaner`, etc.
- [ ] The `frostx init --force` should not overwrite the existing `frostx.toml`, but only update the `id` field if a
  config file is already present, to avoid losing existing configuration.
- [ ] Add a new rule option to only run once, hence, the whole rule can be marked as completed after the first
  successful run.
- [ ] Architecture change: currently, config is applied to all actions, extend to allow for different parameters per
  action, which override global defaults. E.g. to back up to different servers.
- [ ] Tracking the completion of actions is currently done via index, which is not ideal, when the toml file changes.
  Better would be to add additional metadata to the state, e.g. a hash of the rule and action, or maybe just entire
  `frostx.toml`, to detect changes and reset completion if needed.
- [ ] Fix all `cargo clippy --all-targets -- -D warnings -W clippy::pedantic`
- [ ] if the same file is included twice, then skip it the second+ time
- [ ] update last scan, when project was successfully loaded
- [ ] beautiful init questionaire, like with npm init, nice ascii art, etc.
- [ ] compact --help for all subcommands
- [ ] if two different UUID state point to same directory, delete the one which is not active anymore 