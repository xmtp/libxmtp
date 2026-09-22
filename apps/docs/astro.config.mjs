import { unified } from "@astrojs/markdown-remark";
import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";
import rehypeMermaid from "rehype-mermaid";
import starlightLinksValidator from "starlight-links-validator";
import starlightLlmsTxt from "starlight-llms-txt";

import { referencePlugins, referenceSidebar } from "./api-references.mjs";
import { examplePlugins } from "./scripts/example-config.mjs";
import {
  preserveMermaidSource,
  restoreMermaidLanguage,
} from "./scripts/llms-diagrams.mjs";
import { searchRanking } from "./scripts/search-config.mjs";

function guidePages(section, pages) {
  return pages.split(" ").map((page) => ({ slug: `${section}/${page}` }));
}

export default defineConfig({
  site: "https://self-hosted-docs.xmtp.to",
  base: "/",
  trailingSlash: "always",
  markdown: {
    processor: unified({
      rehypePlugins: [
        preserveMermaidSource,
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
        restoreMermaidLanguage,
      ],
    }),
  },
  integrations: [
    starlight({
      title: "Build with XMTP",
      pagefind: { ranking: searchRanking },
      description: "Build secure messaging with XMTP and a backend you run.",
      logo: {
        src: "./src/assets/home/xmtp-logo.svg",
        replacesTitle: true,
      },
      favicon: "/x-mark-blue-lightmode.png",
      customCss: ["./src/styles/custom.css"],
      editLink: {
        baseUrl: "https://github.com/xmtp/libxmtp/edit/self-hosted/apps/docs/",
      },
      lastUpdated: true,
      expressiveCode: {
        themes: ["github-dark", "github-light"],
        plugins: examplePlugins(),
      },
      components: {
        ThemeProvider: "./src/components/ThemeProvider.astro",
        ThemeSelect: "./src/components/ThemeSelect.astro",
        Hero: "./src/components/Hero.astro",
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
            content: "https://self-hosted-docs.xmtp.to/xmtp-og-card.jpeg",
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
            "quickstart install run-the-backend push-configuration migrate-to-self-hosted",
          ),
        },
        {
          label: "Deploy",
          items: guidePages(
            "deploy",
            "overview fly railway aws-ecs kubernetes",
          ),
          collapsed: true,
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
          label: "Tools",
          items: guidePages("tools", "cli web-chat"),
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
          // Keep homepage prompts and checked code in the developer export.
          // Other disclosures retain the existing compact export behavior.
          minify: { details: false },
          customSelectors: {
            all: [
              ".twoslash-popup-container",
              ".twoslash-error-box",
              ".llms-rendered-diagram",
              ".sl-anchor-link",
            ],
            small: ["details:not(.home-disclosure)"],
          },
          promote: ["get-started/**", "sdk/**"],
          exclude: ["reference/**", "specs/**"],
          customSets: [
            {
              label: "Developer guide",
              description:
                "SDK guides, app examples, and deployment instructions. Start here for app development.",
              paths: [
                "index",
                "get-started/**",
                "deploy/**",
                "sdk/**",
                "content-types/**",
                "agents/**",
                "tools/**",
                "protocol/**",
                "reference/limits",
              ],
            },
            {
              label: "Specs",
              description:
                "Protocol requirements. Load only for protocol or implementation work.",
              paths: ["specs/**"],
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
