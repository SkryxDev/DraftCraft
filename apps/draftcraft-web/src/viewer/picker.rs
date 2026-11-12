//! Files for the viewer: the browser's file picker and files dropped on the page, read in the
//! background and handed over on the next frame.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::JsCast as _;
use web_sys::js_sys;

const INPUT_ID: &str = "draftcraft_viewer_input";
/// What the picker offers: drawings, and the SHX fonts they name.
const ACCEPT: &str = ".dwg,.dxf,.shx";
/// Most files taken from one pick.
const MAX_PICKED: u32 = 16;
/// Largest file opened on a phone.
pub const MAX_FILE_BYTES: u64 = 256 << 20;

/// A file read in the browser (name, contents), or why it couldn't be read.
pub type Upload = Result<(String, Vec<u8>), String>;

#[derive(Clone)]
pub struct Inbox {
    files: Rc<RefCell<Vec<Upload>>>,
    ctx: egui::Context,
}

impl Inbox {
    pub fn new(ctx: &egui::Context) -> Self {
        Inbox { files: Rc::default(), ctx: ctx.clone() }
    }

    /// Show the browser's file picker; chosen files arrive through [`Self::take`].
    pub fn pick(&self) {
        if let Err(e) = self.show_picker() {
            self.push(Err(format!("The file picker is not available ({e})")));
        }
    }

    fn show_picker(&self) -> Result<(), String> {
        let document = web_sys::window().and_then(|w| w.document()).ok_or("no browser document")?;
        let body = document.body().ok_or("the page has no body")?;
        if let Some(old) = document.get_element_by_id(INPUT_ID) {
            old.remove();
        }
        let input = document
            .create_element("input")
            .map_err(js_error)?
            .dyn_into::<web_sys::HtmlInputElement>()
            .map_err(|_| "could not create a file input".to_string())?;
        input.set_type("file");
        input.set_accept(ACCEPT);
        input.set_multiple(true);
        input.set_id(INPUT_ID);
        input.set_attribute("style", "display:none").map_err(js_error)?;
        let (inbox, chosen) = (self.clone(), input.clone());
        let on_change = wasm_bindgen::closure::Closure::once_into_js(move || {
            if let Some(files) = chosen.files() {
                for i in 0..files.length().min(MAX_PICKED) {
                    if let Some(file) = files.get(i) {
                        inbox.read(file);
                    }
                }
            }
            chosen.remove();
        });
        input.set_onchange(Some(on_change.unchecked_ref()));
        body.append_child(&input).map_err(js_error)?;
        input.click();
        Ok(())
    }

    /// Read the files dropped on the page this frame.
    pub fn take_dropped(&self, ctx: &egui::Context) {
        for f in ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files)) {
            if let Some(file) = f.web_file() {
                self.read(file.clone());
            }
        }
    }

    fn read(&self, file: web_sys::File) {
        let name = file.name();
        let size = file.size();
        let len = if size.is_finite() && size >= 0.0 { size as u64 } else { u64::MAX };
        if len > MAX_FILE_BYTES {
            return self.push(Err(format!("{name}: larger than the {} MB the viewer opens", MAX_FILE_BYTES >> 20)));
        }
        let inbox = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let r = match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                Ok(buf) => Ok((name, js_sys::Uint8Array::new(&buf).to_vec())),
                Err(e) => Err(format!("{name}: {}", js_error(e))),
            };
            inbox.push(r);
        });
    }

    fn push(&self, upload: Upload) {
        if let Ok(mut files) = self.files.try_borrow_mut() {
            files.push(upload);
        }
        self.ctx.request_repaint();
    }

    /// What arrived since the last frame.
    pub fn take(&self) -> Vec<Upload> {
        self.files.try_borrow_mut().map(|mut f| std::mem::take(&mut *f)).unwrap_or_default()
    }
}

fn js_error(e: wasm_bindgen::JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}
