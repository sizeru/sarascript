# This is an HTML Pre-Processor for rust.

## What are some use cases?
- Adding a navbar/footer/etc. to every page
- Inserting the same meta tags in the \<head\> on every page
- As a library in a custom rust webserver
- Pre-processing a directory on your local computer and copying the processed dir to a production computer

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