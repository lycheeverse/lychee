# Shell Completions for lychee

This directory contains shell completion scripts for lychee, enabling tab-completion for commands, options, and arguments in your shell.

## Quick Reference

Generate completions for your shell:

```bash
lychee --generate complete-bash        # Bash
lychee --generate complete-elvish      # Elvish
lychee --generate complete-fish        # Fish
lychee --generate complete-powershell  # PowerShell
lychee --generate complete-zsh         # Zsh
```

## Automatic Installation

If you installed lychee through a package manager (Homebrew, apt, etc.), shell completions should already be installed and working.

## Manual Installation

### Bash

**User installation:**
```bash
dir="${XDG_CONFIG_HOME:-$HOME/.config}/bash_completion"
mkdir -p "$dir"
lychee --generate complete-bash > "$dir/lychee.bash"
```

Then source it in your `~/.bashrc` or `~/.bash_profile`:
```bash
source "$dir/lychee.bash"
```

**System-wide installation:**
```bash
# On Linux
sudo lychee --generate complete-bash > /usr/share/bash-completion/completions/lychee

# On macOS with Homebrew
lychee --generate complete-bash > $(brew --prefix)/etc/bash_completion.d/lychee
```

### Elvish

```bash
dir="${XDG_CONFIG_HOME:-$HOME/.config}/elvish/lib"
mkdir -p "$dir"
lychee --generate complete-elvish > "$dir/lychee.elv"
```

Then add the following to your `~/.elvish/rc.elv`:
```elvish
use lychee
```

### Fish

**User installation:**
```bash
dir="${XDG_CONFIG_HOME:-$HOME/.config}/fish/completions"
mkdir -p "$dir"
lychee --generate complete-fish > "$dir/lychee.fish"
```

Fish will automatically load the completions on next shell start.

**System-wide installation:**
```bash
# On Linux
sudo lychee --generate complete-fish > /usr/share/fish/vendor_completions.d/lychee.fish

# On macOS with Homebrew
lychee --generate complete-fish > $(brew --prefix)/share/fish/vendor_completions.d/lychee.fish
```

### PowerShell

**Windows:**

Generate the completion file:
```powershell
lychee --generate complete-powershell | Out-File -Encoding UTF8 _lychee.ps1
```

Then add to your PowerShell profile:
```powershell
# Find your profile location
echo $PROFILE

# Add this line to your profile
. C:\Path\To\_lychee.ps1
```

**Linux/macOS with PowerShell:**
```bash
lychee --generate complete-powershell > ~/.config/powershell/_lychee.ps1
```

Add to your profile (`~/.config/powershell/Microsoft.PowerShell_profile.ps1`):
```powershell
. ~/.config/powershell/_lychee.ps1
```

### Zsh

**Recommended approach:**

```zsh
dir="$HOME/.zsh-completions"
mkdir -p "$dir"
lychee --generate complete-zsh > "$dir/_lychee"
```

Add to your `~/.zshrc`:
```zsh
fpath=($HOME/.zsh-completions $fpath)
autoload -Uz compinit && compinit
```

**System-wide installation:**
```bash
# On Linux
sudo lychee --generate complete-zsh > /usr/local/share/zsh/site-functions/_lychee

# On macOS with Homebrew
lychee --generate complete-zsh > $(brew --prefix)/share/zsh/site-functions/_lychee
```

**Alternative (slower, not recommended for daily use):**

You can generate completions on-the-fly by adding this to `~/.zshrc`:
```zsh
source <(lychee --generate complete-zsh)
```

Note: This is easier to set up but slower, adding startup time to your shell.

## Pre-generated Files

This directory contains pre-generated completion files for convenience:

| File | Shell | Description |
|------|-------|-------------|
| `lychee.bash` | Bash | Bash completion script |
| `lychee.elv` | Elvish | Elvish completion module |
| `lychee.fish` | Fish | Fish completion script |
| `_lychee.ps1` | PowerShell | PowerShell completion script |
| `_lychee` | Zsh | Zsh completion function |

These files are regenerated automatically as part of the release process and can be copied directly to your completion directory.

## Troubleshooting

### Completions not working after installation

**Bash:** Make sure you've sourced your `~/.bashrc` or started a new shell session.

**Fish:** Fish loads completions automatically. Try `fish_update_completions` or restart your shell.

**Zsh:** Ensure the directory is in your `$fpath` and you've run `compinit`. Check with:
```zsh
echo $fpath
```

**PowerShell:** Verify your execution policy allows running scripts:
```powershell
Get-ExecutionPolicy
# If restricted, run:
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

### Updating completions

After upgrading lychee, regenerate completions to get the latest options:
```bash
lychee --generate complete-<your-shell> > <completion-file>
```
