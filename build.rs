use anyhow::{
    Context,
    Result,
    anyhow,
};

use inwelling::*;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env,
    fmt::Debug,
    fs::File,
    path::{Path, PathBuf},
    process::Command,
};

type Json = serde_json::value::Value;

const UTF8_PATH: &'static str = "path should be valid UTF-8 string.";
const PKG_NAME_IS_STR: &'static str = "pkg name should be str.";

fn check_os( map: &serde_json::Map<String,Json> ) -> Result<bool> {
    if let Some( os ) = map .get("os") {
        let os = os.as_str().context( "os name should be str." )?;
        Ok( match_os( os ))
    } else {
        Ok( true )
    }
}

fn match_os( name: &str ) -> bool {
    match name {
        "android"   => if cfg!( target_os = "android"   ) {true} else {false},
        "dragonfly" => if cfg!( target_os = "dragonfly" ) {true} else {false},
        "freebsd"   => if cfg!( target_os = "freebsd"   ) {true} else {false},
        "ios"       => if cfg!( target_os = "ios"       ) {true} else {false},
        "linux"     => if cfg!( target_os = "linux"     ) {true} else {false},
        "macos"     => if cfg!( target_os = "macos"     ) {true} else {false},
        "netbsd"    => if cfg!( target_os = "netbsd"    ) {true} else {false},
        "openbsd"   => if cfg!( target_os = "openbsd"   ) {true} else {false},
        "windows"   => if cfg!( target_os = "windows"   ) {true} else {false},
        "unix"      => if cfg!(              unix       ) {true} else {false},
        _           => false,
    }
}

#[derive( Debug )]
pub struct LibInfo {
    link_paths    : RefCell<Vec<String>>,
    include_paths : RefCell<Vec<String>>,
    headers       : RefCell<Vec<String>>,
    specs         : HashMap<String,Json>,
}

impl LibInfo {
    fn new( specs: HashMap<String,Json> ) -> Self {
        LibInfo {
            link_paths    : RefCell::default(),
            include_paths : RefCell::default(),
            headers       : RefCell::default(),
            specs         ,
        }
    }

    fn probe( &self, pkg_name: &str, scan_incdir: bool ) -> Result<()> {
        let probed_ex = self
            .probe_via_pkgconf( pkg_name, scan_incdir )
            .unwrap_or_else( |_| self.probe_via_search( pkg_name, scan_incdir ));

        if scan_incdir {
            self.include_paths.borrow_mut().push( self.get_includedir( &probed_ex )? );
        }

        if let Some( spec ) = self.specs.get( pkg_name ) {
            let include_dir = self.get_includedir( &probed_ex )?;

            if let Some( object ) = spec.as_object() {
                if !scan_incdir {
                    object
                        .get( "headers" )
                        .and_then( |headers| headers.as_array() )
                        .and_then( |headers| headers
                            .iter()
                            .try_for_each( |header| -> Result<()> {
                                if let Some( header ) = header.as_str() {
                                    self.headers.borrow_mut().push(
                                        Path::new( &include_dir )
                                            .join( header )
                                            .to_str()
                                            .context( UTF8_PATH )?
                                            .to_owned()
                                    )
                                }
                                Ok(())
                            })
                            .ok()
                        ).context( UTF8_PATH )?;

                    if !probed_ex.pkgconf_ok() {
                        if let Some( dependencies ) = object.get( "dependencies" ) {
                            match dependencies {
                                Json::Array( dependencies ) => for pkg_name in dependencies {
                                    self.probe( pkg_name.as_str().context( PKG_NAME_IS_STR )?, false )?;
                                },
                                Json::Object( dependencies ) => for (pkg_name, dep) in dependencies {
                                    let dep = dep.as_object().context("named dependency should be object.")?;
                                    if check_os( dep )? {
                                        self.probe( pkg_name, false )?;
                                    }
                                },
                                _ => return Err( anyhow!( "invalid dependencies." )),
                            }
                        }
                    }
                }

                if let Some( dependencies ) = object.get( "header-dependencies" ) {
                    match dependencies {
                        Json::Array( dependencies ) => for pkg_name in dependencies {
                            self.probe( pkg_name.as_str().context( PKG_NAME_IS_STR )?, true )?;
                        },
                        Json::Object( dependencies ) => for (pkg_name, dep) in dependencies {
                            let dep = dep.as_object().context("named dependency should be object.")?;
                            if check_os( dep )? {
                                self.probe( pkg_name, true )?;
                            }
                        },
                        _ => return Err( anyhow!( "invalid header-dependencies." )),
                    }
                }
            }
        }
        Ok(())
    }

    fn probe_via_pkgconf( &self, pkg_name: &str, scan_incdir: bool ) -> Result<ProbedEx> {
        let mut cfg = pkg_config::Config::new();
        cfg.cargo_metadata( true );

        let mut pc_file_names = vec![ pkg_name ];

        if let Some( spec ) = self.specs.get( pkg_name ) {
            let object = spec.as_object().expect("clib specs should be a json map.");
            if let Some( pc_alias ) = object.get("pc-alias") {
                pc_alias
                    .as_array()
                    .expect("pc-alias should be array.")
                    .iter()
                    .for_each( |pc| {
                        pc_file_names.push( pc.as_str().expect( ".pc file name should be str." ));
                    });
            }
        }

        let mut names = pc_file_names.into_iter();
        let (library, pc_name) = loop {
            if let Some( name ) = names.next() {
                if let Ok( library ) = cfg.probe( name ) {
                    break (library, name.to_owned() );
                }
            } else {
                return Err( anyhow!( "failed to locate .pc file" ));
            }
        };

        if !scan_incdir {
            library.link_paths
                .into_iter()
                .map( |path| path.to_str().expect( UTF8_PATH ).to_owned() )
                .for_each( |link_path| self.link_paths.borrow_mut().push( link_path ));

            library.include_paths
                .into_iter()
                .map( |path| path.to_str().expect( UTF8_PATH ).to_owned() )
                .for_each( |include_path| self.include_paths.borrow_mut().push( include_path ));
        }

        Ok( ProbedEx::PcName( pc_name ))
    }

    fn probe_via_search( &self, pkg_name: &str, scan_incdir: bool ) -> ProbedEx {
        if let Some( object ) = self.specs
            .get( pkg_name )
            .unwrap()
            .as_object()
        {
            object
                .get( "exe" )
                .and_then( |exe| exe.as_array() )
                .map( |executable_names| -> ProbedEx {
                    for name in executable_names {
                        let name = name.as_str().expect("exe names should be str.");
                        let output = Command::new( if cfg!(unix) { "which" } else { "where" })
                            .arg( name ).output();
                        match output {
                            Ok( output ) => {
                                let s = output.stdout.as_slice();
                                if s.is_empty() {
                                    continue;
                                }
                                let cmd_path = Path::new( std::str::from_utf8( s )
                                    .expect( UTF8_PATH )
                                    .trim_end() );

                                let parent = cmd_path.parent()
                                    .expect("executable should not be found in root directory.");
                                assert_eq!( parent.file_name().expect( UTF8_PATH ), "bin" );
                                let prefix = parent.parent()
                                    .expect("bin should not be found in root directory.");
                                let include_base = prefix.join("include");

                                let guess_include = object
                                    .get("includedir")
                                    .and_then( |includedirs| includedirs.as_array() )
                                    .and_then( |dirs| Some( dirs.iter().map( |dir| dir.as_str().expect( "include dir should be str." ))))
                                    .and_then( |dirs| {
                                        for dir in dirs {
                                            let dir = include_base.join( dir );
                                            if dir.exists() {
                                                return Some( dir.to_str().expect( UTF8_PATH ).to_owned() );
                                            }
                                        }
                                        Some( include_base.to_str().expect( UTF8_PATH ).to_owned() )
                                    })
                                    .expect("include_path");

                                if !scan_incdir {
                                    self.link_paths.borrow_mut().push( prefix.join("lib").to_str().expect( UTF8_PATH ).to_owned() );
                                    println!( "cargo:rustc-link-search=native={}/lib", prefix.to_str().expect( UTF8_PATH ));
                                    emit_cargo_meta_for_libs( &prefix, object.get( "libs" ).expect( "metadata should contain libs" ));
                                    object.get( "libs-private" ).map( |libs| emit_cargo_meta_for_libs( &prefix, libs ));
                                }

                                return ProbedEx::IncDir( guess_include );
                            },
                            Err(_) => continue,
                        }
                    }
                    panic!("failed to locate executable");
                })
                .expect("lib probed via search.")
        } else {
            panic!("failed to search lib.");
        }
    }

    fn get_includedir( &self, probe_ex: &ProbedEx ) -> Result<String> {
        match probe_ex {
            ProbedEx::PcName( pc_name ) => {
                let exe = env::var( "PKG_CONFIG" ).unwrap_or_else( |_| "pkg-config".to_owned() );
                let mut cmd = Command::new( exe );
                cmd.args( &[ &pc_name, "--variable", "includedir" ]);

                let output = cmd.output()?;
                let result = Ok( std::str::from_utf8( output.stdout.as_slice() )?
                    .trim_end().to_owned() );
                result
            },
            ProbedEx::IncDir( includedir ) => {
                let path = Path::new( &includedir );
                assert!( path.exists() );
                Ok( format!( "{}", path.display() ))
            },
        }
    }
}

fn emit_cargo_meta_for_libs( prefix: &Path, value: &Json ) {
    let lib_path = prefix.join("lib");

    if let Some( object ) = value.as_object() {
        'values:
        for value in object.values() {
            let lib_names = value.as_array().expect("names of libs should be an array.");
            for lib_name in lib_names {
                let lib_name = lib_name.as_str().expect( "lib name should be str." );
                if lib_path.join( lib_name ).exists() {
                    println!( "cargo:rustc-link-lib={}", get_link_name( lib_name ));
                    continue 'values;
                }
            }
            panic!("lib should be found in {:?} directory.", lib_path );
        }
    } else if let Some( lib_names ) = value.as_array() {
        for lib_name in lib_names {
            let lib_name = lib_name.as_str().expect("lib name should be str.");
            if lib_path.join( lib_name ).exists() {
                println!( "cargo:rustc-link-lib={}", get_link_name( lib_name ));
            } else {
                panic!( "failed to locate {}", lib_name );
            }
        }
    }
}

fn get_link_name( lib_name: &str ) -> &str {
    let start = if lib_name.starts_with( "lib" ) { 3 } else { 0 };
    match lib_name.rfind('.') {
        Some( dot ) => &lib_name[ start..dot ],
        None => &lib_name[ start.. ],
    }
}

enum ProbedEx {
    IncDir( String ),
    PcName( String ),
}

impl ProbedEx {
    fn pkgconf_ok( &self ) -> bool {
        match self {
            ProbedEx::IncDir(_)  => false,
            ProbedEx::PcName(_)  => true,
        }
    }
}

fn generate_nothing() {
    let out_path = PathBuf::from( env::var( "OUT_DIR" ).expect( "$OUT_DIR should exist." ));
    File::create( out_path.join( "bindings.rs" )).expect( "an empty bindings.rs generated." );
}

fn main() {
    let (specs, builds) = inwelling( Opts::default() )
        .sections
        .into_iter()
        .fold(( HashMap::<String,Json>::new(), HashSet::<String>::new() ), |(mut specs, mut builds), section| {
            section.metadata
                .as_object()
                .map( |obj| {
                    obj .get( "spec" )
                        .and_then( |spec| spec.as_object() )
                        .map( |spec| spec.iter()
                            .for_each( |(key,value)| { specs.insert( key.clone(), value.clone() ); }));
                    obj .get( "build" )
                        .and_then( |build| build.as_array() )
                        .map( |build_list| build_list.iter()
                            .for_each( |pkg| { pkg.as_str().map( |pkg| { builds.insert( pkg.to_owned() ); }); }));
                });
           (specs, builds)
        });

    if builds.is_empty() {
        generate_nothing();
        return;
    }

    #[cfg( target_os = "freebsd" )]
    env::set_var( "PKG_CONFIG_ALLOW_CROSS", "1" );

    let lib_info_all = LibInfo::new( specs );

    builds.iter().for_each( |pkg_name| {
        if !pkg_name.is_empty() {
            lib_info_all.probe( pkg_name, false ).unwrap();
        }
    });

    if lib_info_all.headers.borrow().is_empty() {
        generate_nothing();
        return;
    }

    let mut builder = bindgen::Builder::default()
        .generate_comments( false )
    ;

    for header in lib_info_all.headers.borrow().iter() {
        builder = builder.header( header );
    }
    for path in lib_info_all.include_paths.borrow().iter() {
        let opt = format!( "-I{}", path );
        builder = builder.clang_arg( &opt );
    }

    let bindings = builder.generate().expect( "bindgen builder constructed." );
    let out_path = PathBuf::from( env::var( "OUT_DIR" ).expect( "$OUT_DIR should exist." ));
    bindings.write_to_file( out_path.join( "bindings.rs" )).expect( "bindings.rs generated." );
}
