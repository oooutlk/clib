#[cfg( features = "libtk" )]
pub unsafe fn use_tk() {
    clib::Tk_Init( std::ptr::null_mut() );
}
