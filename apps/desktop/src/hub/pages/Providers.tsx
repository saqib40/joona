import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Confirm } from "@/components/ui/confirm";
import { Group, PageHeader, Row } from "@/components/ui/layout";
import { SarvamSheet } from "../components/SarvamSheet";
import { listNames } from "../format";
import { languageName } from "../languages";
import { SARVAM, SARVAM_PRIVACY, useProviders, useSarvam } from "../providers";
import { type Settings, useHub } from "../store";
import { AiModels } from "./AiModels";
import { SpeechToText } from "./SpeechToText";

const TITLE = "Providers";
const DESCRIPTION =
  "Cloud services and AI models you choose, with your own API keys. Nothing goes to a service you haven't turned on.";

export function Providers() {
  const settings = useHub((s) => s.settings);
  const loaded = useProviders((s) => s.keys !== null);
  useEffect(() => void useProviders.getState().refresh(), []);
  if (!settings || !loaded) return <PageHeader title={TITLE} description={DESCRIPTION} />;
  return (
    <>
      <PageHeader title={TITLE} description={DESCRIPTION} />
      <Sarvam settings={settings} />
      <SpeechToText settings={settings} />
      <AiModels settings={settings} />
    </>
  );
}

function Sarvam({ settings }: { settings: Settings }) {
  const { consented, hasKey, ready } = useSarvam();
  const [sheet, setSheet] = useState<"add" | "replace" | null>(null);
  const [testing, setTesting] = useState(false);
  const sent = settings.languages.speak.filter((l) => l.engine === "sarvam").map((l) => languageName(l.code));

  const test = async () => {
    setTesting(true);
    const failure = await useProviders.getState().test(SARVAM);
    setTesting(false);
    if (failure) toast.error("Sarvam didn't work", { description: failure.message });
    else toast.success("Sarvam accepted your key");
  };

  const usedFor =
    sent.length > 0
      ? `Turning ${listNames(sent)} into text, and translating text when your languages need it.`
      : consented
        ? "Translating text when your languages need it. Send a language to Sarvam on the Languages page to have it turned into text."
        : "Nothing yet. Turn Sarvam on here, or pick it for a language on the Languages page.";

  return (
    <>
      <Group
        title="Sarvam AI"
        description="Speech to text, translation and English letters for 22 Indian languages. Based in India."
        action={
          ready ? (
            <Badge tone="cloud">On</Badge>
          ) : consented ? (
            <Badge tone="danger">Needs a key</Badge>
          ) : (
            <Badge>Off</Badge>
          )
        }
      >
        <Row title="API key" description={hasKey ? "Saved in Windows Credential Manager." : "Not added yet."}>
          {hasKey ? (
            <>
              <Button disabled={testing} onClick={() => void test()}>
                {testing ? "Testing…" : "Test"}
              </Button>
              <Button onClick={() => setSheet("replace")}>Replace</Button>
            </>
          ) : (
            <Button variant="primary" onClick={() => setSheet("add")}>
              {consented ? "Add key" : "Turn on Sarvam"}
            </Button>
          )}
        </Row>
        <Row title="Used for" description={usedFor}>
          <Button variant="ghost" onClick={() => useHub.getState().navigate("languages")}>
            Languages
          </Button>
        </Row>
        <Row
          title="Privacy"
          description="Sarvam only receives what's listed above, and the bar shows a cloud badge each time."
        >
          <Button variant="ghost" onClick={() => void openUrl(SARVAM_PRIVACY)}>
            Privacy policy
            <ExternalLink />
          </Button>
        </Row>
        {(consented || hasKey) && (
          <Row
            title="Turn off Sarvam"
            description="Deletes the key from this PC. Languages you sent to Sarvam go back to this PC."
          >
            <Confirm
              title="Turn off Sarvam?"
              description="Joona deletes your Sarvam key from this PC and stops sending it anything. Languages you sent to Sarvam are turned into text on this PC again."
              confirm="Turn off"
              onConfirm={() => void useProviders.getState().remove(SARVAM)}
            >
              <Button variant="quiet">Turn off…</Button>
            </Confirm>
          </Row>
        )}
      </Group>
      <SarvamSheet
        open={sheet !== null}
        replace={sheet === "replace"}
        onOpenChange={(open) => !open && setSheet(null)}
      />
    </>
  );
}
