pub unsafe fn use_tcl() {
    clib::Tcl_FindExecutable( std::ptr::null() );
}
