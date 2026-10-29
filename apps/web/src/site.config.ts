// The one place that knows where Mozhi lives on the internet.
export const REPO_URL = "https://github.com/Kamal007OLica/mozhi";

export const SITE = {
  // TODO: switch to the custom domain once there is one.
  url: "https://mozhi-khaki.vercel.app",
  repo: REPO_URL,
  // No release is published yet, so link the list (/releases/latest would 404).
  releases: `${REPO_URL}/releases`,
  issues: `${REPO_URL}/issues/new/choose`,
  contributing: `${REPO_URL}/blob/main/CONTRIBUTING.md`,
  license: `${REPO_URL}/blob/main/LICENSE`,
  buildFromSource: `${REPO_URL}#building-from-source`,
  version: "0.1.0",
};
