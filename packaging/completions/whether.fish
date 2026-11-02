# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_whether_global_optspecs
    string join \n c/config= d/display= l/location= units= offline v/verbose watch attach h/help V/version
end

function __fish_whether_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_whether_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_whether_using_subcommand
    set -l cmd (__fish_whether_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c whether -n "__fish_whether_needs_command" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_needs_command" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_needs_command" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_needs_command" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_needs_command" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_needs_command" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_needs_command" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_needs_command" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_needs_command" -s V -l version -d 'Print version'
complete -c whether -n "__fish_whether_needs_command" -f -a "run" -d 'Run the selected display (the default)'
complete -c whether -n "__fish_whether_needs_command" -f -a "tui" -d 'Run the terminal display'
complete -c whether -n "__fish_whether_needs_command" -f -a "gui" -d 'Run the desktop display'
complete -c whether -n "__fish_whether_needs_command" -f -a "line" -d 'Print one line for status bars'
complete -c whether -n "__fish_whether_needs_command" -f -a "json" -d 'Print JSON for scripts'
complete -c whether -n "__fish_whether_needs_command" -f -a "config" -d 'Inspect and create the configuration'
complete -c whether -n "__fish_whether_needs_command" -f -a "displays" -d 'List the compiled-in displays'
complete -c whether -n "__fish_whether_needs_command" -f -a "locate" -d 'Search for a place; print a `[[location]]` snippet or add it'
complete -c whether -n "__fish_whether_needs_command" -f -a "providers" -d 'List providers: authentication, capabilities, limits and attribution'
complete -c whether -n "__fish_whether_needs_command" -f -a "fetch" -d 'Fetch once and print the snapshot as JSON (debugging)'
complete -c whether -n "__fish_whether_needs_command" -f -a "history" -d 'Print a place\'s history: normals, records and the last days as observed'
complete -c whether -n "__fish_whether_needs_command" -f -a "climate" -d 'How chosen dates were over chosen years, for planning a trip (--location may be any place\'s name)'
complete -c whether -n "__fish_whether_needs_command" -f -a "daemon" -d 'Fetch in the background and serve displays on a socket (use them with --attach)'
complete -c whether -n "__fish_whether_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c whether -n "__fish_whether_using_subcommand run" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand run" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand run" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand run" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand run" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand run" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand run" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand run" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand run" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand tui" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand tui" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand tui" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand tui" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand tui" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand tui" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand tui" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand tui" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand tui" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand gui" -l screenshot -d 'Render one frame headlessly to this PNG and exit' -r -F
complete -c whether -n "__fish_whether_using_subcommand gui" -l size -d 'Screenshot size, e.g. 1920x1080' -r
complete -c whether -n "__fish_whether_using_subcommand gui" -l fixture -d 'Screenshot this cached snapshot (a cache JSON file) instead of the cache' -r -F
complete -c whether -n "__fish_whether_using_subcommand gui" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand gui" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand gui" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand gui" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand gui" -l fullscreen -d 'Full screen without decorations (like `kiosk = true`)'
complete -c whether -n "__fish_whether_using_subcommand gui" -l compact -d 'Screenshot the compact window'
complete -c whether -n "__fish_whether_using_subcommand gui" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand gui" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand gui" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand gui" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand gui" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand line" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand line" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand line" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand line" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand line" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand line" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand line" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand line" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand line" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand json" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand json" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand json" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand json" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand json" -l snapshot -d 'The raw snapshot instead of the view model'
complete -c whether -n "__fish_whether_using_subcommand json" -l compact -d 'One line instead of pretty JSON'
complete -c whether -n "__fish_whether_using_subcommand json" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand json" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand json" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand json" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand json" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -f -a "init" -d 'Write the commented example configuration'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -f -a "check" -d 'Validate the configuration (exit 2 on errors)'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -f -a "show" -d 'Print the effective configuration (secrets masked)'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -f -a "path" -d 'Print where configuration files are read from'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -f -a "schema" -d 'Print the JSON Schema for editor completion'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -f -a "strings" -d 'List every text key and its default'
complete -c whether -n "__fish_whether_using_subcommand config; and not __fish_seen_subcommand_from init check show path schema strings help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -l force -d 'Overwrite an existing file'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from check" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from check" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from check" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from check" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from check" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from check" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from check" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from check" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from check" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -l resolved -d 'Show units and presentation as resolved for the default display'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from path" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from path" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from path" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from path" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from path" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from path" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from path" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from path" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from path" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from schema" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from schema" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from schema" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from schema" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from schema" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from schema" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from schema" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from schema" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from schema" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from strings" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from strings" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from strings" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from strings" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from strings" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from strings" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from strings" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from strings" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from strings" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "init" -d 'Write the commented example configuration'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "check" -d 'Validate the configuration (exit 2 on errors)'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "show" -d 'Print the effective configuration (secrets masked)'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "path" -d 'Print where configuration files are read from'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "schema" -d 'Print the JSON Schema for editor completion'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "strings" -d 'List every text key and its default'
complete -c whether -n "__fish_whether_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c whether -n "__fish_whether_using_subcommand displays" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand displays" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand displays" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand displays" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand displays" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand displays" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand displays" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand displays" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand displays" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand locate" -l country -d 'ISO 3166-1 alpha-2 country code, e.g. US' -r
complete -c whether -n "__fish_whether_using_subcommand locate" -l count -d 'How many results to show' -r
complete -c whether -n "__fish_whether_using_subcommand locate" -l name -d 'The name to give the added location' -r
complete -c whether -n "__fish_whether_using_subcommand locate" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand locate" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand locate" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand locate" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand locate" -l zip -d 'Search a postal code (needs --country)'
complete -c whether -n "__fish_whether_using_subcommand locate" -l add -d 'Append the first result to the configuration file'
complete -c whether -n "__fish_whether_using_subcommand locate" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand locate" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand locate" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand locate" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand locate" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -f -a "models" -d 'List the weather models `[providers.open-meteo] models` accepts, marking those with data for the first location'
complete -c whether -n "__fish_whether_using_subcommand providers; and not __fish_seen_subcommand_from models help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from models" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from models" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from models" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from models" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from models" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from models" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from models" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from models" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from models" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from help" -f -a "models" -d 'List the weather models `[providers.open-meteo] models` accepts, marking those with data for the first location'
complete -c whether -n "__fish_whether_using_subcommand providers; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c whether -n "__fish_whether_using_subcommand fetch" -l provider -d 'Use only this provider (openweather, open-meteo)' -r
complete -c whether -n "__fish_whether_using_subcommand fetch" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand fetch" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand fetch" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand fetch" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand fetch" -l raw -d 'Print only the snapshot, without the attempts'
complete -c whether -n "__fish_whether_using_subcommand fetch" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand fetch" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand fetch" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand fetch" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand fetch" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand history" -l days -d 'How many past days (1–31; default: `history_days`)' -r
complete -c whether -n "__fish_whether_using_subcommand history" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand history" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand history" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand history" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand history" -l json -d 'Print JSON: the history as shown and as received'
complete -c whether -n "__fish_whether_using_subcommand history" -l station -d 'List the ACIS stations near the place instead, to choose one for `[history.acis] station`'
complete -c whether -n "__fish_whether_using_subcommand history" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand history" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand history" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand history" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand history" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand climate" -l from -d 'The first date: MM-DD (the next one) or YYYY-MM-DD' -r
complete -c whether -n "__fish_whether_using_subcommand climate" -l to -d 'The last date (default: the first; at most 31 days on)' -r
complete -c whether -n "__fish_whether_using_subcommand climate" -l years -d 'The years to look back over, e.g. 1995-2024 (default: the 30 before this one)' -r
complete -c whether -n "__fish_whether_using_subcommand climate" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand climate" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand climate" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand climate" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand climate" -l json -d 'Print JSON: the query, the view and the answer as received'
complete -c whether -n "__fish_whether_using_subcommand climate" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand climate" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand climate" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand climate" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand climate" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand daemon" -s c -l config -d 'Use only this configuration file' -r -F
complete -c whether -n "__fish_whether_using_subcommand daemon" -s d -l display -d 'Display to run (see `whether displays`)' -r
complete -c whether -n "__fish_whether_using_subcommand daemon" -s l -l location -d 'Location to show first, by name or 1-based index' -r
complete -c whether -n "__fish_whether_using_subcommand daemon" -l units -d 'Override the unit system' -r -f -a "metric\t'°C, km/h, hPa, mm, km'
imperial\t'°F, mph, inHg, in, mi'"
complete -c whether -n "__fish_whether_using_subcommand daemon" -l offline -d 'Never use the network; show cached data'
complete -c whether -n "__fish_whether_using_subcommand daemon" -s v -l verbose -d 'More logging (-v info, -vv debug, -vvv trace)'
complete -c whether -n "__fish_whether_using_subcommand daemon" -l watch -d 'line and json: print again on every change'
complete -c whether -n "__fish_whether_using_subcommand daemon" -l attach -d 'Show data from a running `whether daemon` instead of fetching'
complete -c whether -n "__fish_whether_using_subcommand daemon" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "run" -d 'Run the selected display (the default)'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "tui" -d 'Run the terminal display'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "gui" -d 'Run the desktop display'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "line" -d 'Print one line for status bars'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "json" -d 'Print JSON for scripts'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "config" -d 'Inspect and create the configuration'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "displays" -d 'List the compiled-in displays'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "locate" -d 'Search for a place; print a `[[location]]` snippet or add it'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "providers" -d 'List providers: authentication, capabilities, limits and attribution'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "fetch" -d 'Fetch once and print the snapshot as JSON (debugging)'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "history" -d 'Print a place\'s history: normals, records and the last days as observed'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "climate" -d 'How chosen dates were over chosen years, for planning a trip (--location may be any place\'s name)'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "daemon" -d 'Fetch in the background and serve displays on a socket (use them with --attach)'
complete -c whether -n "__fish_whether_using_subcommand help; and not __fish_seen_subcommand_from run tui gui line json config displays locate providers fetch history climate daemon help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c whether -n "__fish_whether_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "init" -d 'Write the commented example configuration'
complete -c whether -n "__fish_whether_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "check" -d 'Validate the configuration (exit 2 on errors)'
complete -c whether -n "__fish_whether_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "show" -d 'Print the effective configuration (secrets masked)'
complete -c whether -n "__fish_whether_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "path" -d 'Print where configuration files are read from'
complete -c whether -n "__fish_whether_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "schema" -d 'Print the JSON Schema for editor completion'
complete -c whether -n "__fish_whether_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "strings" -d 'List every text key and its default'
complete -c whether -n "__fish_whether_using_subcommand help; and __fish_seen_subcommand_from providers" -f -a "models" -d 'List the weather models `[providers.open-meteo] models` accepts, marking those with data for the first location'
