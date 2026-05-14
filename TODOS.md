# Todos

List of features, I would like to add, in no particular order. Not a roadmap, just a brain dump of ideas.

- [ ] for include files there should be a templating option, e.g. setting the toplevel `template` data structure, e.g.
  with a question that gets asked during `frost init`, which then includes the template file and fills in the variables.
- [ ] `frost init` should do a short questionnaire to set up the initial `frostx.toml`
- [ ] architecture change: there should be an Action Manager to which actions are registered, since they have their
  name, which can be returned, we do not need then to hardcode the existing action name and match arms
- [ ] Either create a documentation how to setup or custom command for this, once a day, if any project needs to be run,
  run the pipeline in a terminal init, if no rule is eligible, sleep for 24h and check again. This way users might just
  add frostx to their bashrc and forget about it. State that the tool run on that day may be saved to the state dir.
- [ ] On tar gz/archive action, archive the whole folder an replace the project folder with that archive, instead of
  creating a new archive besides the project folder.
- [ ] When implemented above, frostx should modify the state path to point to the archive, and it should still be able
  to run further pipelines, hence, some tar streaming implementation may be used.
- [ ] For verification that the project is put to the remote folder, we must always check the integrity of the upload,
  if the backend does not support that natively, we need to download the uploaded file and compare checksums, or
  something like that. Might mark then the completion to only do this once. For the ssh upload, not rsync, this should
  be done via remote checksum command, if supported, download only as fallback.
- [ ] Add a global option to shift the time of the last activity, e.g. for testing purposes, or if the user would like
  to force pipeline execution now.
- [ ] The cleanup action should be agnostic of the files present, e.g. instead of hardcoding `target/` and
  `node_modules/`, it should check e.g. the presence of `Cargo.toml` and `package.json` and only then clean those
  folders. The user might hardcode some paths in addition to these options (which should also be enabled/disabled via
  config). Probably best here a trait such that easily new cleaners can be added, e.g. `RustCleaner`, `NodeCleaner`,
  `PythonCleaner`, etc.
- [ ] The `frostx init --force` should not overwrite the existing `frostx.toml`, but only update the `id` field if a
  config file is already present, to avoid losing existing configuration.
- [ ] `git` and `jj` actions currently change the modification time of the project folder - not ideal - need to find a
  solution.
- [ ] Add a new rule option to only run once, hence, the whole rule can be marked as completed after the first
  successful run.
- [ ] Architecture change: currently, config is applied to all actions, extend to allow for different parameters per
  action, which override global defaults. E.g. to back up to different servers.
- [ ] Tracking the completion of actions is currently done via index, which is not ideal, when the toml file changes.
  Better would be to add additional metadata to the state, e.g. a hash of the rule and action, or maybe just entire
  `frostx.toml`, to detect changes and reset completion if needed.
- [ ] all easy registration for new urls besides rsync, ssh, via trait abstraction?
- [ ] Fix all `cargo clippy --all-targets -- -D warnings -W clippy::pedantic`
- [ ] if the same file is included twice, then skip it the second+ time
- [ ] update last scan, when project was successfully loaded