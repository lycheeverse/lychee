
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'lychee' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'lychee'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'lychee' {
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Configuration file to use  [default: lychee.toml]')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Configuration file to use  [default: lychee.toml]')
            [CompletionResult]::new('--files-from', '--files-from', [CompletionResultType]::ParameterName, 'Read input filenames from the given file or stdin (if path is ''-'')')
            [CompletionResult]::new('--extensions', '--extensions', [CompletionResultType]::ParameterName, 'A list of file extensions. Files not matching the specified extensions are skipped')
            [CompletionResult]::new('--default-extension', '--default-extension', [CompletionResultType]::ParameterName, 'This is the default file extension that is applied to files without an extension.')
            [CompletionResult]::new('--max-cache-age', '--max-cache-age', [CompletionResultType]::ParameterName, 'Discard all cached requests older than this duration')
            [CompletionResult]::new('--cache-exclude-status', '--cache-exclude-status', [CompletionResultType]::ParameterName, 'A list of status codes that will be excluded from the cache')
            [CompletionResult]::new('--archive', '--archive', [CompletionResultType]::ParameterName, 'Specify the use of a specific web archive. Can be used in combination with `--suggest`')
            [CompletionResult]::new('-m', '-m', [CompletionResultType]::ParameterName, 'Maximum number of allowed redirects')
            [CompletionResult]::new('--max-redirects', '--max-redirects', [CompletionResultType]::ParameterName, 'Maximum number of allowed redirects')
            [CompletionResult]::new('--max-retries', '--max-retries', [CompletionResultType]::ParameterName, 'Maximum number of retries per request')
            [CompletionResult]::new('--min-tls', '--min-tls', [CompletionResultType]::ParameterName, 'Minimum accepted TLS Version')
            [CompletionResult]::new('--max-concurrency', '--max-concurrency', [CompletionResultType]::ParameterName, 'Maximum number of concurrent network requests')
            [CompletionResult]::new('--host-concurrency', '--host-concurrency', [CompletionResultType]::ParameterName, 'Default maximum concurrent requests per host (default: 10)')
            [CompletionResult]::new('--host-request-interval', '--host-request-interval', [CompletionResultType]::ParameterName, 'Minimum interval between requests to the same host (default: 50ms)')
            [CompletionResult]::new('-T', '-T ', [CompletionResultType]::ParameterName, 'Number of threads to utilize. Defaults to number of cores available to the system')
            [CompletionResult]::new('--threads', '--threads', [CompletionResultType]::ParameterName, 'Number of threads to utilize. Defaults to number of cores available to the system')
            [CompletionResult]::new('-u', '-u', [CompletionResultType]::ParameterName, 'User agent')
            [CompletionResult]::new('--user-agent', '--user-agent', [CompletionResultType]::ParameterName, 'User agent')
            [CompletionResult]::new('-s', '-s', [CompletionResultType]::ParameterName, 'Only test links with the given schemes (e.g. https). Omit to check links with any other scheme. At the moment, we support http, https, file, and mailto')
            [CompletionResult]::new('--scheme', '--scheme', [CompletionResultType]::ParameterName, 'Only test links with the given schemes (e.g. https). Omit to check links with any other scheme. At the moment, we support http, https, file, and mailto')
            [CompletionResult]::new('--include', '--include', [CompletionResultType]::ParameterName, 'URLs to check (supports regex). Has preference over all excludes')
            [CompletionResult]::new('--exclude', '--exclude', [CompletionResultType]::ParameterName, 'Exclude URLs and mail addresses from checking. The values are treated as regular expressions')
            [CompletionResult]::new('--exclude-file', '--exclude-file', [CompletionResultType]::ParameterName, 'Deprecated; use `--exclude-path` instead')
            [CompletionResult]::new('--exclude-path', '--exclude-path', [CompletionResultType]::ParameterName, 'Exclude paths from getting checked. The values are treated as regular expressions')
            [CompletionResult]::new('--remap', '--remap', [CompletionResultType]::ParameterName, 'Remap URI matching pattern to different URI')
            [CompletionResult]::new('--fallback-extensions', '--fallback-extensions', [CompletionResultType]::ParameterName, 'Automatically append file extensions to `file://` URIs for non-existing paths')
            [CompletionResult]::new('--index-files', '--index-files', [CompletionResultType]::ParameterName, 'Resolve local directory links to specified index files within the directory')
            [CompletionResult]::new('-H', '-H ', [CompletionResultType]::ParameterName, 'Set custom header for requests')
            [CompletionResult]::new('--header', '--header', [CompletionResultType]::ParameterName, 'Set custom header for requests')
            [CompletionResult]::new('-a', '-a', [CompletionResultType]::ParameterName, 'A List of accepted status codes for valid links')
            [CompletionResult]::new('--accept', '--accept', [CompletionResultType]::ParameterName, 'A List of accepted status codes for valid links')
            [CompletionResult]::new('-t', '-t', [CompletionResultType]::ParameterName, 'Website timeout in seconds from connect to response finished')
            [CompletionResult]::new('--timeout', '--timeout', [CompletionResultType]::ParameterName, 'Website timeout in seconds from connect to response finished')
            [CompletionResult]::new('-r', '-r', [CompletionResultType]::ParameterName, 'Minimum wait time in seconds between retries of failed requests')
            [CompletionResult]::new('--retry-wait-time', '--retry-wait-time', [CompletionResultType]::ParameterName, 'Minimum wait time in seconds between retries of failed requests')
            [CompletionResult]::new('-X', '-X ', [CompletionResultType]::ParameterName, 'Request method')
            [CompletionResult]::new('--method', '--method', [CompletionResultType]::ParameterName, 'Request method')
            [CompletionResult]::new('--base', '--base', [CompletionResultType]::ParameterName, 'Deprecated; use `--base-url` instead')
            [CompletionResult]::new('-b', '-b', [CompletionResultType]::ParameterName, 'Base URL used to resolve relative URLs in local files. Example: <https://example.com>')
            [CompletionResult]::new('--base-url', '--base-url', [CompletionResultType]::ParameterName, 'Base URL used to resolve relative URLs in local files. Example: <https://example.com>')
            [CompletionResult]::new('--root-dir', '--root-dir', [CompletionResultType]::ParameterName, 'Root directory to use when checking absolute links in local files. Must be an absolute path')
            [CompletionResult]::new('--basic-auth', '--basic-auth', [CompletionResultType]::ParameterName, 'Basic authentication support. E.g. `http://example.com username:password`')
            [CompletionResult]::new('--github-token', '--github-token', [CompletionResultType]::ParameterName, 'GitHub API token to use when checking github.com links, to avoid rate limiting')
            [CompletionResult]::new('-o', '-o', [CompletionResultType]::ParameterName, 'Output file of status report')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Output file of status report')
            [CompletionResult]::new('--mode', '--mode', [CompletionResultType]::ParameterName, 'Set the output display mode. Determines how results are presented in the terminal')
            [CompletionResult]::new('-f', '-f', [CompletionResultType]::ParameterName, 'Output format of final status report')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format of final status report')
            [CompletionResult]::new('--generate', '--generate', [CompletionResultType]::ParameterName, 'Generate special output (e.g. the man page) instead of performing link checking')
            [CompletionResult]::new('--cookie-jar', '--cookie-jar', [CompletionResultType]::ParameterName, 'Read and write cookies using the given file')
            [CompletionResult]::new('-p', '-p', [CompletionResultType]::ParameterName, 'Preprocess input files')
            [CompletionResult]::new('--preprocess', '--preprocess', [CompletionResultType]::ParameterName, 'Preprocess input files')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Set verbosity level; more output per occurrence (e.g. `-v` or `-vv`)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Set verbosity level; more output per occurrence (e.g. `-v` or `-vv`)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Less output per occurrence (e.g. `-q` or `-qq`)')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Less output per occurrence (e.g. `-q` or `-qq`)')
            [CompletionResult]::new('-n', '-n', [CompletionResultType]::ParameterName, 'Do not show progress bar. This is recommended for non-interactive shells (e.g. for continuous integration)')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Do not show progress bar. This is recommended for non-interactive shells (e.g. for continuous integration)')
            [CompletionResult]::new('--host-stats', '--host-stats', [CompletionResultType]::ParameterName, 'Show per-host statistics at the end of the run')
            [CompletionResult]::new('--cache', '--cache', [CompletionResultType]::ParameterName, 'Use request cache stored on disk at `.lycheecache`')
            [CompletionResult]::new('--dump', '--dump', [CompletionResultType]::ParameterName, 'Don''t perform any link checking. Instead, dump all the links extracted from inputs that would be checked')
            [CompletionResult]::new('--dump-inputs', '--dump-inputs', [CompletionResultType]::ParameterName, 'Don''t perform any link extraction and checking. Instead, dump all input sources from which links would be collected')
            [CompletionResult]::new('--suggest', '--suggest', [CompletionResultType]::ParameterName, 'Suggest link replacements for broken links, using a web archive. The web archive can be specified with `--archive`')
            [CompletionResult]::new('-i', '-i', [CompletionResultType]::ParameterName, 'Proceed for server connections considered insecure (invalid TLS)')
            [CompletionResult]::new('--insecure', '--insecure', [CompletionResultType]::ParameterName, 'Proceed for server connections considered insecure (invalid TLS)')
            [CompletionResult]::new('--offline', '--offline', [CompletionResultType]::ParameterName, 'Only check local files and block network requests')
            [CompletionResult]::new('-E', '-E ', [CompletionResultType]::ParameterName, 'Exclude all private IPs from checking. Equivalent to `--exclude-private --exclude-link-local --exclude-loopback`')
            [CompletionResult]::new('--exclude-all-private', '--exclude-all-private', [CompletionResultType]::ParameterName, 'Exclude all private IPs from checking. Equivalent to `--exclude-private --exclude-link-local --exclude-loopback`')
            [CompletionResult]::new('--exclude-private', '--exclude-private', [CompletionResultType]::ParameterName, 'Exclude private IP address ranges from checking')
            [CompletionResult]::new('--exclude-link-local', '--exclude-link-local', [CompletionResultType]::ParameterName, 'Exclude link-local IP address range from checking')
            [CompletionResult]::new('--exclude-loopback', '--exclude-loopback', [CompletionResultType]::ParameterName, 'Exclude loopback IP address range and localhost from checking')
            [CompletionResult]::new('--include-mail', '--include-mail', [CompletionResultType]::ParameterName, 'Also check email addresses')
            [CompletionResult]::new('--include-fragments', '--include-fragments', [CompletionResultType]::ParameterName, 'Enable the checking of fragments in links')
            [CompletionResult]::new('--skip-missing', '--skip-missing', [CompletionResultType]::ParameterName, 'Skip missing input files (default is to error if they don''t exist)')
            [CompletionResult]::new('--no-ignore', '--no-ignore', [CompletionResultType]::ParameterName, 'Do not skip files that would otherwise be ignored by ''.gitignore'', ''.ignore'', or the global ignore file')
            [CompletionResult]::new('--hidden', '--hidden', [CompletionResultType]::ParameterName, 'Do not skip hidden directories and files')
            [CompletionResult]::new('--include-verbatim', '--include-verbatim', [CompletionResultType]::ParameterName, 'Find links in verbatim sections like `pre`- and `code` blocks')
            [CompletionResult]::new('--glob-ignore-case', '--glob-ignore-case', [CompletionResultType]::ParameterName, 'Ignore case when expanding filesystem path glob inputs')
            [CompletionResult]::new('--require-https', '--require-https', [CompletionResultType]::ParameterName, 'When HTTPS is available, treat HTTP links as errors')
            [CompletionResult]::new('--include-wikilinks', '--include-wikilinks', [CompletionResultType]::ParameterName, 'Check WikiLinks in Markdown files, this requires specifying --base-url')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
