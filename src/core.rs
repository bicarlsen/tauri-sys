//! Common functionality
use serde::{Serialize, de::DeserializeOwned};
use serde_wasm_bindgen as swb;

pub use channel::{Channel, Message};
pub use resource::Resource;

pub async fn invoke<T>(command: &str, args: impl Serialize) -> T
where
    T: DeserializeOwned,
{
    let value = inner::invoke(command, swb::to_value(&args).unwrap()).await;
    swb::from_value(value).unwrap()
}

pub async fn invoke_result<T, E>(command: &str, args: impl Serialize) -> Result<T, E>
where
    T: DeserializeOwned,
    E: DeserializeOwned,
{
    inner::invoke_result(command, swb::to_value(&args).unwrap())
        .await
        .map(|val| swb::from_value(val).unwrap())
        .map_err(|err| swb::from_value(err).unwrap())
}

pub fn convert_file_src(file_path: impl AsRef<str>) -> String {
    inner::convert_file_src(file_path.as_ref(), "asset")
        .as_string()
        .unwrap()
}

pub fn convert_file_src_with_protocol(
    file_path: impl AsRef<str>,
    protocol: impl AsRef<str>,
) -> String {
    inner::convert_file_src(file_path.as_ref(), protocol.as_ref())
        .as_string()
        .unwrap()
}

pub fn is_tauri() -> bool {
    inner::is_tauri()
}

mod resource {
    use super::invoke;
    use serde::Serialize;

    #[derive(Clone)]
    /// A Rust backed resource.
    pub struct Resource {
        rid: u64,
    }

    impl Resource {
        pub fn new(rid: u64) -> Self {
            Self { rid }
        }

        pub fn rid(&self) -> u64 {
            self.rid
        }

        /// Destroy the resource.
        pub async fn close(self) {
            #[derive(Serialize)]
            struct Args {
                rid: u64,
            }

            invoke::<()>("plugin:resources|close", Args { rid: self.rid }).await;
        }
    }
}

mod channel {
    use super::inner;
    use futures::{Stream, StreamExt, channel::mpsc};
    use serde::{Deserialize, Serialize, de::DeserializeOwned, ser::SerializeStruct};
    use wasm_bindgen::{JsValue, prelude::Closure};

    #[derive(derive_more::Deref, Deserialize, Debug)]
    pub struct Message<T> {
        id: usize,

        #[deref]
        message: T,
    }

    impl<T> Message<T> {
        pub fn id(&self) -> usize {
            self.id
        }
    }

    // TODO: Could cause memory leak because handler is never released.
    #[derive(Debug)]
    pub struct Channel<T> {
        id: usize,
        rx: mpsc::UnboundedReceiver<Message<T>>,
    }

    impl<T> Channel<T> {
        pub fn new() -> Self
        where
            T: DeserializeOwned + 'static,
        {
            let (tx, rx) = mpsc::unbounded::<Message<T>>();
            let closure = Closure::<dyn FnMut(JsValue)>::new(move |raw| {
                let _ = tx.unbounded_send(serde_wasm_bindgen::from_value(raw).unwrap());
            });

            let id = inner::transform_callback(&closure, false);
            closure.forget();

            Channel { id, rx }
        }

        pub fn id(&self) -> usize {
            self.id
        }
    }

    impl<T> Serialize for Channel<T> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let mut map = serializer.serialize_struct("Channel", 2)?;
            map.serialize_field("__TAURI_CHANNEL_MARKER__", &true)?;
            map.serialize_field("id", &self.id)?;
            map.end()
        }
    }

    impl<T> Stream for Channel<T> {
        type Item = T;

        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            self.rx
                .poll_next_unpin(cx)
                .map(|item| item.map(|value| value.message))
        }
    }
}

mod inner {
    use wasm_bindgen::{
        JsValue,
        prelude::{Closure, wasm_bindgen},
    };

    #[wasm_bindgen(module = "/src/core.js")]
    extern "C" {
        pub async fn invoke(cmd: &str, args: JsValue) -> JsValue;
        #[wasm_bindgen(js_name = "invoke", catch)]
        pub async fn invoke_result(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
        #[wasm_bindgen(js_name = "convertFileSrc")]
        pub fn convert_file_src(filePath: &str, protocol: &str) -> JsValue;
        #[wasm_bindgen(js_name = "transformCallback")]
        pub fn transform_callback(callback: &Closure<dyn FnMut(JsValue)>, once: bool) -> usize;
        #[wasm_bindgen(js_name = "isTauri")]
        pub fn is_tauri() -> bool;
    }
}
