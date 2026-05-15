# Todos

List of features, I would like to add, in no particular order. Not a roadmap, just a brain dump of ideas.

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