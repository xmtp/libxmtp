import { defineConfig } from "astro/config";
import { unified } from "@astrojs/markdown-remark";
import starlight from "@astrojs/starlight";
import rehypeMermaid from "rehype-mermaid";
import starlightLlmsTxt from "starlight-llms-txt";
import starlightLinksValidator from "starlight-links-validator";
import { examplePlugins } from "./scripts/example-config.mjs";
import { referencePlugins, referenceSidebar } from "./api-references.mjs";
import { searchRanking } from "./scripts/search-config.mjs";

function guidePages(section, pages) {
  return pages.split(" ").map((page) => ({ slug: `${section}/${page}` }));
}

export default defineConfig({
  site: "https://docs.xmtp.org",
  base: "/",
  trailingSlash: "always",
  markdown: {
    processor: unified({
      rehypePlugins: [
        [
          rehypeMermaid,
          {
            strategy: "img-svg",
            dark: true,
            mermaidConfig: {
              theme: "neutral",
              themeVariables: { background: "transparent" },
            },
          },
        ],
      ],
    }),
  },
  integrations: [
    starlight({
      title: "Build with XMTP",
      pagefind: { ranking: searchRanking },
      description: "Build secure messaging with XMTP and a backend you run.",
      logo: {
        light: "./src/assets/logomark-light-purple.png",
        dark: "./src/assets/logomark-dark-purple.png",
        replacesTitle: true,
      },
      favicon: "/x-mark-blue-lightmode.png",
      customCss: ["./src/styles/custom.css"],
      editLink: {
        baseUrl: "https://github.com/xmtp/libxmtp/edit/main/apps/docs/",
      },
      lastUpdated: true,
      expressiveCode: {
        themes: ["github-dark", "github-light"],
        plugins: examplePlugins(),
      },
      components: {
        Header: "./src/components/Header.astro",
        Footer: "./src/components/Footer.astro",
        PageTitle: "./src/components/PageTitle.astro",
        MarkdownContent: "./src/components/MarkdownContent.astro",
      },
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/xmtp/libxmtp",
        },
      ],
      head: [
        {
          tag: "meta",
          attrs: {
            property: "og:image",
            content: "https://docs.xmtp.org/xmtp-og-card.jpeg",
          },
        },
        {
          tag: "script",
          attrs: {
            async: true,
            src: "https://plausible.io/js/pa-Ck_8Ax5131WKaGcJv5brM.js",
          },
        },
        {
          tag: "script",
          content:
            "window.plausible=window.plausible||function(){(plausible.q=plausible.q||[]).push(arguments)},plausible.init=plausible.init||function(i){plausible.o=i||{}};\n  plausible.init()",
        },
      ],
      sidebar: [
        {
          label: "Get started",
          items: guidePages(
            "get-started",
            "quickstart install run-the-backend migrate-to-self-hosted",
          ),
        },
        {
          label: "SDK guide",
          items: guidePages(
            "sdk",
            "client signer inboxes conversations send-messages read stream sync backups groups consent disappearing-messages delete-messages push-notifications debug use-signatures extend-identity-model",
          ),
        },
        {
          label: "Content types",
          items: guidePages(
            "content-types",
            "overview attachments reactions replies read-receipts actions-and-intents transactions transaction-refs group-updates markdown",
          ),
          collapsed: true,
        },
        {
          label: "Agents",
          items: guidePages(
            "agents",
            "quickstart events middleware filters context deploy",
          ),
          collapsed: true,
        },
        {
          label: "Protocol",
          items: guidePages(
            "protocol",
            "overview security envelope-types topics epochs intents cursors identity",
          ),
          collapsed: true,
        },
        {
          label: "Specs",
          items: [{ autogenerate: { directory: "specs" } }],
          collapsed: true,
        },
        {
          label: "Reference",
          items: [
            { slug: "reference/limits" },
            { slug: "reference/error-glossary" },
            ...referenceSidebar,
            {
              label: "Swift SDK",
              link: "/reference/swift/documentation/xmtpios/",
            },
            { label: "Kotlin SDK", link: "/reference/kotlin/" },
            { label: "Rust", link: "/rust/" },
          ],
          collapsed: true,
        },
      ],
      plugins: [
        ...referencePlugins,
        starlightLlmsTxt({
          customSelectors: {
            all: [".twoslash-popup-container", ".twoslash-error-box"],
          },
          promote: ["get-started/**", "sdk/**"],
          exclude: ["reference/**"],
          customSets: [
            {
              label: "Developer guide",
              paths: [
                "index",
                "get-started/**",
                "sdk/**",
                "content-types/**",
                "agents/**",
                "protocol/**",
                "specs/**",
                "reference/limits",
              ],
            },
          ],
        }),
        starlightLinksValidator({
          errorOnInvalidHashes: true,
          sameSitePolicy: "validate",
          exclude: ["/reference/swift/**", "/reference/kotlin/**", "/rust/**"],
        }),
      ],
    }),
  ],
});
