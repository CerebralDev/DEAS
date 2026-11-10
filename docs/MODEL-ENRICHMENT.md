# Selected model metadata and thumbnails

Choose resources in Organize library, then Refresh metadata & thumbnails. The native service performs exact hash lookup sequentially, caches bounded Civitai thumbnails, reports matched/unmatched/failed outcomes and supports cancellation and explicit retry. Missing credentials and unavailable matches remain visible. Opening the library only reads local caches.

Author descriptions, example-image settings and user notes are separate. Fetched recommendations are informational and never applied to generation. Folders, tags, pins, notes and preferred thumbnails survive refresh. Existing cache records remain readable. No model weights are downloaded.

Depends on model-library; native commands native_model_scan_selected, native_model_scan_cancel and native_model_assets are unchanged from the installed custom build. Tests live beside downloads_models_scan and downloads_model_assets. Network fixtures do not establish live provider or Linux UI acceptance.
