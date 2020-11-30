# Toubleshooting Guide

This document describes common edge-cases and workarounds for checking links to various sites.  
Please add your own findings and send us a pull request if you can.  

## Unexpected Status Codes

Some websites don't respond with a `200` (OK) status code.  
Instead they might send `204` (No Content), `206` (Partial Content), or 
[something else entirely](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/418).

If you run into such issues you can work around that by providing a custom  
list of accepted status codes, such as `--accept 200,204,206`.

## Website Expects Custom Headers

Some sites expect one or more custom headers to return a valid response.  
For example, crates.io expects a `Accept: text/html` header or else it  
will [return a 404](https://github.com/rust-lang/crates.io/issues/788).  

To fix that you can pass additional headers like so: `--headers "accept=text/html"`.  
You can use that argument multiple times to add more headers.  
