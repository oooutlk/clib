# Purpose

Use metadata embeded in Cargo.toml files to generate bindings to C libraries.

It is an all-in-one solution, an alternative to `-sys` crates, each of which
should have generated bindings separately for one single C library.

# Requirements

1. C libraries can be compiled with `bindgen`'s default configuration.

2. C libraries provides pkg-config file, or its installation is consistent with
the assumption of this crate.

3. The downstream crates of this library should invoke `inwelling::to( "clib" )`
in their `build.rs`, and `cargo add --build inwelling` is required.

# Usage demonstration: step-by-step explanation of tk library metadata

Sample Cargo.toml files of tcl and tk libraries are in `examples/` folder. Let's
take tk's metadata for demonstration.

## Section of metadata

```toml
[package.metadata.inwelling.clib]
build = ["tk86"]
```

All metadata are located in section `[package.metadata.inwelling.clib]`, because
this crate depends on crate lib, which utilizes
[crate inwelling](https://crates.io/crates/inwelling) to collect metadata from
downstream users.

The value `build = ["tk86"]` indicates that crate tk is asking crate clib to
build the C library of tk86.

## Section of build specification

```toml
[package.metadata.inwelling.clib.spec.tk86]
```

This section provides necessary information to probe library's headers,
include/link paths and C libraries that needs to link against, with or without
`pkg-config`.

Note that "spec" is placed between "clib" and "tk86" in the section name. It is
the specification for building tk86, but not asking crate clib to build. As
mentioned above, it is `build = ["tk86"]` which makes clib to build tk86.

## Enumerating names of pkg-config file

```toml
[package.metadata.inwelling.clib.spec.tk86]
pc-alias = ["tk"]
```

This value tells `pkg-config` to find `tk.pc` if `tk86.pc` does not exist.

## Enumerating header files of tk library

```toml
[package.metadata.inwelling.clib.spec.tk86]
headers = ["tk.h"]
```

This value tells crate bindgen that "tk.h" is the header file of tk86 library.

## Enumerating dependencies

```toml
[package.metadata.inwelling.clib.spec.tk86]
dependencies = ["tcl86"]
```

This value specifies tcl86 as the dependency of tk86. The crate clib will probe
tcl86 and its dependencies recusively, if any( actually, none in this example).

The metadata of tcl86 is in section
`[package.metadata.inwelling.clib.spec.tcl86]` of `examples/tcl/Cargo.toml`
which will be collected by crate inwelling as well.

## Enumerating possible executable file names

```toml
[package.metadata.inwelling.clib.spec.tk86]
exe = ["wish86", "wish"]
```

The value `exe = ["wish86", "wish"]` tells that the executable file name of tk
shell may be "wish86" or "wish". It is optional, only used if `pkg-config` is
missing or failed to probe library. The crate clib will try to locate the
executable file and expect the include and link path to be
"../include/{some-dir-in-includedir}" and "../lib" respectively. Note that
"wish86.exe" and "wish.exe" are not necessary for Windows.

## Enumerating possible include paths

```toml
[package.metadata.inwelling.clib.spec.tk86]
includedir = ["tk8.6", "tk"]
```

The value `includedir = ["tk8.6", "tk"]` tells the possible names of include
path under "../include" which is mentioned previously. If none of the path
exists, the include path will be expected to be "../include". It is optional,
only used if `pkg-config` is missing or failed to probe library.

## Importing extra include paths

```toml
[package.metadata.inwelling.clib.spec.tk86]
header-dependencies = ["x11"]
```

This value tells crate clib to add include paths of x11 and its
"header-dependencies" recusively, if any, to tk's.

## The libs section

```toml
[package.metadata.inwelling.clib.spec.tk86.libs]
tk = ["libtk86.so", "libtk8.6.so", "libtk.so", "libtk86.a", "libtk.a", "libtk86.dll.a", "libtk.dll.a", "tk86t.dll", "tk86t.lib"]
tkstub = ["libtkstub86.a", "libtkstub8.6.a", "libtkstub.a", "tkstub86.lib"]
```

The value `tk = [..]` enumerates possible library file names that need link
against. Note that the key name "tk" is for human readability only.

Once file of some name has been found under link path, the crate clib will stop
searching and emit "cargo:rustc-link-lib={the-stripped-name}" to cargo. For
example, if "libtk86.so" has been found, the prefix "lib" and suffix ".so" will
be stripped and "cargo:rustc-link-lib=tk86" will be emitted.

# Global namespace

All generated functions, types and constants are in the root namespace of this
crate. You can prefix them with `clib::`, e.g.
`clib::Tcl_Init()`/`clib::Tk_Init()`, or `use clib::*;` and use
`Tcl_Init()`/`Tk_Init()` directly. On the other hand, traditional "-sys" crates
of tcl-sys and tk-sys generate `tcl_sys::Tcl_Init()` and `tk_sys::Tk_Init()`
respectively.

# Caveat

## Windows does not support pkg-config well

This crate works well if pkg-config works on the OS. When it is not the case,
e.g. on Windows, crate clib will search the installation location with some
assumptions, and the search may fail in a larger probability.

# License

Under Apache License 2.0 or MIT License, at your will.
