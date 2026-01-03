# Shell Completions for lychee

lychee comes with built-in support for generating shell completions.
This unlocks tab-completion for commands, options, and arguments in your shell!

## Installation

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
