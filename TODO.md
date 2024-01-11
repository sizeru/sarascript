# TODO
## Main features... should be implemented roughly in order:
- [x] Implement getting in core library (this creates LIBRARY). A simple "get" request is enough for the MVP.
- [x] Create SERVER version by creating daemon and CONFIG
- [ ] Create STANDALONE version by providing a command line interface. This can come later
- [ ] Create CLIENT version somehow??? (I'm thinking WASM)
- [ ] Allow deployment as single-page-application. (This allows a page to look and run like a native app on iOS). Maybe this can be done through a config flag. (You could theoretically have some code which only updates what changes, since browser caching doesn't work in this case).
- [ ] Add the function: `preprocess_from_template(template_path);` (working title), and also syntax for defining a template. Will allow less repetition for html files.
- [ ] Add the date modified when sending static files to encourage browser caching.
- [ ] Determine whether a sarascript file is dynamic or not, and cache it appropriately.

## Prepping for v1.0.0
- [ ] Document all public functions and structs
- [ ] cargo clippy
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
- **Features:** Maybe networking should be an explicit feature. The simplest web server will host all files locally. Networking is really for distributed systems.
- **Remove dependencies:** pest should probably be replaced with something in-house. Tokio maybe replaced with mio. These are all incredible libraries, but I'd like extreme control over memory usage. the httparse crate is excellent at this
- **No std:** Make cross compiling to weird platforms easier.
- **Do I need to parse the DOM?** Maybe... when you get into more complex functions such as templates, can a find and replace really suffice? I feel like since elements could be added in arbitrary places, and I still want to maintain the FEEL of writing pure html, I have to go beyond just a find-and-replace algorithm


## Focus
- v1 is focused on stability
- next release is focused on optimization