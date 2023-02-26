# Introduction

**cltodo** is a CLI application implementing a TODO list. It is developed for fast, flexible and simple use.

# Installation

Just run

```console
cargo install cltodo
```

to install it directly from crates.io.

If you are on linux, you can install it by downloading the package on releases and running

```console
sudo dpkg -i cltodo_0.1.1-1_amd64.deb
```

# Quick start

Each entry on the list has three possible priorities: "normal", "important" or "critical".

Add an entry with:

```console
~$ cltodo add "Align with Alice about refatoring foo.rs" -p "important"
```

Get all entries with:

```console
~$ cltodo get
#3: CRITICAL : 2023-02-25: Fix tests!!!
#1: IMPORTANT: 2023-02-25: Investigate database performance
#4: NORMAL   : 2023-02-25: Change lighting in some images for the webpage
```

Getting entries has a lot optional arguments available. For example, you can filter by some date using:

```console
~$ cltodo get --from "2023-12-12"
No results found.
```

For an extensive list, run `cltodo get -h `.

```console
~$ cltodo get -h
Queries TODO entries based on the parameters

Usage: cltodo.exe get [OPTIONS]

Options:
  -p, --priority <PRIORITY>  Filters by entries with the given priority [possible values: normal, important, critical]
  -f, --from <FROM>          Filters by entries that are more recent than the given datetime. Inclusive
  -t, --to <TO>              Filters by entries that are older than the given datetime. Inclusive
  -e, --extended             Displays datetimes in extended mode, i.e. with hours, mins, secs and time zone
  -r, --reversed             Reverses the order displayed on the query. The default is more recent entries on the top
  -c, --chronological        Sticks to chronological order sort only, disregarding priority
  -h, --help                 Print help
```
