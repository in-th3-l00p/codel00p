import type { Metadata } from "next";

import {
  DocHeader,
  H2,
  P,
  Ul,
  Ic,
  Strong,
  Table,
} from "@/components/docs/prose";

export const metadata: Metadata = {
  title: "Providers & routing — codel00p docs",
  description:
    "Provider profiles, transports, credential resolution, routing, and policy.",
};

export default function ProvidersPage() {
  return (
    <>
      <DocHeader
        group="Architecture"
        title="Providers & routing"
        lead="codel00p-providers gives the harness one consistent way to call models, without tying project memory to any single vendor."
      />

      <H2>Profiles and transports</H2>
      <P>
        Provider support is two layers. A <Strong>profile</Strong> describes a
        provider&apos;s identity, aliases, API mode, credentials, and model
        catalog. A <Strong>transport</Strong> is the wire adapter for one API
        mode: it converts messages and tools, builds the request, and normalizes
        the response and usage. Sending JSON to a URL is not enough, since
        provider quirks are real, so each mode has its own transport.
      </P>
      <Table
        head={["Provider", "API mode", "Credential"]}
        rows={[
          ["Anthropic", <Ic key="a">anthropic_messages</Ic>, "API key"],
          [
            "OpenAI",
            <span key="o">
              <Ic>responses</Ic>, <Ic>chat_completions</Ic>
            </span>,
            "API key",
          ],
          ["Azure AI Foundry", <Ic key="z">azure_chat_completions</Ic>, "Key + endpoint"],
          ["AWS Bedrock", <Ic key="b">bedrock_converse</Ic>, "AWS credential chain"],
          ["Google Gemini", <Ic key="g">gemini</Ic>, "API key"],
          ["OpenRouter / custom", <Ic key="c">chat_completions</Ic>, "Key + base URL"],
        ]}
      />

      <H2>Credentials</H2>
      <P>
        Credentials are resolved from the environment and never stored in
        memory. Each provider reads a standard variable, for example{" "}
        <Ic>ANTHROPIC_API_KEY</Ic> or <Ic>OPENAI_API_KEY</Ic>, and also accepts a
        namespaced form like <Ic>CODEL00P_PROVIDER_OPENAI_API_KEY</Ic> when you
        want keys scoped to codel00p alone. Explicitly injected credentials take
        precedence over environment ones.
      </P>

      <H2>Routing, policy, and cost</H2>
      <P>
        A resolved route carries more than a URL. It records safe audit metadata,
        and the layer adds policy and accounting on top:
      </P>
      <Ul>
        <li>
          <Strong>Fallback routing</Strong> retries retryable failures across an
          ordered list of routes;
        </li>
        <li>
          <Strong>Policy</Strong> filters providers and models by allowlist and
          required capabilities, with built-in presets selectable through{" "}
          <Ic>--provider-policy-preset</Ic>;
        </li>
        <li>
          <Strong>Cost</Strong> normalizes usage and produces request-priced cost
          estimates with explicit pricing sources;
        </li>
        <li>
          <Strong>Gateways</Strong> are reached by overriding the endpoint with{" "}
          <Ic>--base-url</Ic> for any OpenAI-compatible service.
        </li>
      </Ul>
      <P>
        This keeps inference configurable per project, organization, or gateway
        while audit and policy semantics stay consistent across every surface.
      </P>
    </>
  );
}
