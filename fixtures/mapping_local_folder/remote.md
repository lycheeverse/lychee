This file is intended to be served by a mock server in the tests.

It will be accessed from a URL of:
```
{mock_server}/server/1/2/file.md
```

It is used by both tests which map the whole domain and tests which map
a subpath. For the subpath tests, the mapped subpath is:
```
{mock_server}/server/1/
```

[root](/root)

[up 2](/../../up-up)

[up 3](/../../../up-up-up)

[relative](relative.html)

[sub dir](sub/dir/index.html)

[up](../up-one.html)

[up two](../../up-two.html)

[current page anchor](#self)

[query params](query.html?boop=20)

`encoded$*( )[ ].html`
[encoded](encoded%24%2A%28%20%29%5B%20%5D.html)
