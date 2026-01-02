this file will be tested by mapping the following remote URL into the following local directory:
- remote URL: `https://gist.githubusercontent.com/katrinafyi/daefc003e04b7c2f73cb54615510dce0/raw/`
- local dir: `a/b/ROOT`

this maps only pages which are subpaths of the remote URL.

these links are outside base-url, should STAY remote:
[canonical](https://gist.githubusercontent.com/fully/qualified.html)
[canonical up](https://gist.githubusercontent.com/../fully/qualified/up.html)
[canonical root](https://gist.githubusercontent.com)
[fake canonical](https://gist.githubusercontent.com-fake)

these links should STAY local:
[relative](relative.md)

these links should BECOME remote:
[up](../up)
[very up](../../../../very-up)
[root](/root)
[root-up](/../root-up)

these links should BECOME local:
[a](https://gist.githubusercontent.com/katrinafyi/daefc003e04b7c2f73cb54615510dce0/raw/make-me-local)
