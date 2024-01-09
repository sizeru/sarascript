# TODO
## Main features... should be implemented roughly in order:
- [x] Implement getting in core library (this creates LIBRARY). A simple "get" request is enough for the MVP.
- [x] Create SERVER version by creating daemon and CONFIG
- [ ] Create STANDALONE version by providing a command line interface. This can come later
- [ ] Create CLIENT version somehow??? (I'm thinking WASM)
- [ ] Allow deployment as single-page-application. (This allows a page to look and run like a native app on iOS). Maybe this can be done through a config flag. (You could theoretically have some code which only updates what changes, since browser caching doesn't work in this case).

## Prepping for v1.0.0
- [ ] Document all public functions and structs
- [ ] Run and post benchmarks
- [ ] Get a Windows release running
- [ ] Ensure no known bugs

## Patches... unkown if these have been fixed already:
- [ ] Need to put a little more work into the TLS implementation.
- [ ] Poorly formatted scripts result in the closing of the connection. I can't imagine this being desirable behavior, but quiet failures lead to oversights. Think about how to resolve this.

## Side features and thought dump
- **Fix CSS name mangling**: Sarascript permits the user to dynamically load html files, which will load CSS files. This can result in a nest of CSS which is all present in one singular file. **If and only if CSS writers are not mindful of this issue**, having conflicting css definitions can lead to unexpected results. I believe CSS preprocessors like SCSS have a solution to this issue. Perhaps this crate should provide a method to link with SCSS.
- **Add markdown parsing**: HTML is a great language for designing a business page, something to recruit people. Markdown is the undisputed KING of blogging. I created this tool mainly with the goal of making blogging easier in mind. A user should put some markdown files in a folder and have the program do the rest. Hugo is great great for exactly this, although I would take a different approach that requires less explicit configuration and is a lot more hands-off and resembles the raw HTML+CSS+JS workflow. Regardless, this program needs to link with a markdown-to-HTML converter.
- **Optimize:** The plan is to optimize this program after v1.0.0 is released. Stability first.