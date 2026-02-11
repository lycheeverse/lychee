this file will be tested by mapping the following remote URL into the following local directory:
- remote URL: `https://gist.githubusercontent.com/`
- local dir: `a/b/ROOT`

this maps all pages within a certain domain.

these links should become local:
[canonical](https://gist.githubusercontent.com/fully/qualified.html)
[canonical up](https://gist.githubusercontent.com/../fully/qualified/up.html)
[canonical root](https://gist.githubusercontent.com)

these links should NOT become local:
[fake canonical](https://gist.githubusercontent.com-fake)

these links should stay local:
[relative](relative.md)
[root](/root)
[root-up](/../root-up)
