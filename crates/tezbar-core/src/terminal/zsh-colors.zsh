# Restore the user's startup directory before loading their .zshenv. All later
# startup files (.zprofile, .zshrc, .zlogin) then follow zsh's normal lookup/order.
if [[ ${TEZBAR_USER_ZDOTDIR_SET:-0} == 1 ]]; then
    ZDOTDIR=$TEZBAR_USER_ZDOTDIR
else
    unset ZDOTDIR
fi
unset TEZBAR_USER_ZDOTDIR TEZBAR_USER_ZDOTDIR_SET
if [[ -r ${ZDOTDIR-$HOME}/.zshenv ]]; then
    builtin source "${ZDOTDIR-$HOME}/.zshenv"
fi

if [[ -o interactive ]]; then
    _tezbar_color_default_prompt() {
        # Only replace the stock macOS/zsh prompts. Themes and user prompts win.
        case ${NO_COLOR+x}:$PROMPT in
            ':%n@%m %1~ %# ' | ':%m%# ')
                PROMPT='%F{yellow}%n@%m%f %1~ %F{8}%#%f '
                ;;
        esac
        precmd_functions=(${precmd_functions:#_tezbar_color_default_prompt})
        unfunction _tezbar_color_default_prompt
    }
    typeset -ga precmd_functions
    precmd_functions+=(_tezbar_color_default_prompt)
fi
