# A lightweight declarative language for HTML
SaraScript is a simple declarative language which can be used to extend an HTML document. SaraScript can be used as:
- A static-site generator[1].
- A server-side renderer[1].
- An asynchronous client-side renderer
###### [1]: "Rendering" in a web context is typically just modifying the HTML so that the browser may render it properly
## Use Cases:
Since SaraScript can be used in three distinct ways, this table compares each version:
| Version    | Use Case                  | Pros                                        | Cons                                             |
|------------|---------------------------|---------------------------------------------|--------------------------------------------------|
| Server     | Server-side rendering     | No client scripts & Reduced Server Storage  | Increased Server CPU & Increased Network Traffic |
| Client     | Client-side rendering     | Reduced server storage & Minimal server CPU | Client-side scripts                              |
| Standalone | Static-site generation      | No client scripts & Minimal server CPU      | Larger Server Storage                            |
| Library    | Used in custom webservers | N/a                                         | N/a                                              |

## Example use cases
- Adding a navbar/footer/etc. to every page
- Retrieving and embedding a user's picture into the site
- Generating static web pages

- Inserting the same meta tags in the \<head\> on every page
- As a library in a custom rust webserver
- Pre-processing a directory on your local computer and copying the processed dir to a production computer

## Why not use PHP?
Mainly due to security concerns and ease-of-use. Sarascript is designed to be much simpler to use and much smaller in scope than PHP.

## Give Me More Depth on a Use Case
Imagine that you had a static website that had a few dozen pages of information. In order to make navigation easier, you may want to include a navbar on all of your pages. You likely would like to only have to edit the navbar in one location, and have all your webpages be updated accordingly. You have a few serious options:
1. Using PHP, add an include directive to your html files and either dynamically or statically include a navbar
2. Add a script to your page which modifies the DOM client-side and inserts a navbar 
3. Use a web framework that allows you to insert arbitrary html/css/js server-side.

This library allows you to do the first option, statically, without the reliance on PHP.

## Why not just use PHP?
You could. PHP can generate static files. [Here's a script on Stack Overflow that does exactly that](https://stackoverflow.com/questions/32028857/want-to-render-or-generate-all-php-files-in-a-directory-to-static-html). You would even get more features with PHP. But if you can't use PHP for whatever reason, want to process html server-side, and don't want to lock yourself into a JavaScript framework, then this crate checks all those boxes.


## Features
- Include any number of arbitrary files in your html
- Pre-process either individual files or whole folders at once
- Customize settings so that file extensions determine:
    - Which files should be processed
    - What extension to give a processed file
    - Which files should be ignored
    - Which files should be copied over as it
- Set root dir for includes (include paths can be based on your webserver's root)
- All pre-processor keywords and directives are inside of comments, meaning your raw HTML code is still valid if you're into that (I am).
- Fast enough: Can crank through over 1GB of HTML files per second on my computer. Haven't done solid benchmarking yet.

## Misc

This project practices semantic versioning: https://semver.org/spec/v2.0.0.html

## Name
The hero shrew is a small cute mouse-like creature with an incredibly strong spine. Though they are the size of a deck of cards - they are said to be able to support the weight of an adult male without harm. The animal is native the the DRC. musaraigne is the word for shrew in French - I was unable to find the name in Kituba. 
