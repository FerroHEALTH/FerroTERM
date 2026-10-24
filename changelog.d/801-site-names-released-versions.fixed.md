- The published book and landing page name only released versions. The Docs
  workflow refuses to deploy while the site names a version that is not a
  published release with its `compose.yaml`, and the release workflow
  dispatches the Docs deploy once the release is published, so a reader never
  meets install commands for an image that does not exist yet.
