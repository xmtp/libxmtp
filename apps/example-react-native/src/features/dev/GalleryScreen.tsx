import { Stack, router, useLocalSearchParams } from "expo-router";

import { ChatReplica } from "@/features/dev/ChatReplica";
import { ChatsReplica } from "@/features/dev/ChatsReplica";
import { ComponentsGallery } from "@/features/dev/ComponentsGallery";
import { ConnectReplica } from "@/features/dev/ConnectReplica";
import { galleryCopy } from "@/features/dev/copy";
import { DetailsReplica } from "@/features/dev/DetailsReplica";
import { SettingsReplica } from "@/features/dev/SettingsReplica";
import { FormScreen, FormSection } from "@/ui/Form";
import { NavigationRow } from "@/ui/FormRows";

const sections = {
  components: ComponentsGallery,
  settings: SettingsReplica,
  details: DetailsReplica,
  chats: ChatsReplica,
  chat: ChatReplica,
  connect: ConnectReplica,
} as const;

type SectionName = keyof typeof sections;

function isSection(value: string | undefined): value is SectionName {
  return value !== undefined && value in sections;
}

/**
 * The Debug design gallery: every design-system component in its states,
 * and replicas of reference screens for pixel comparison. Deep link
 * `xmtp-messenger-rn:///dev/gallery?section=<name>`.
 */
export function GalleryScreen() {
  const { section } = useLocalSearchParams<{ section?: string }>();
  if (isSection(section)) {
    const Section = sections[section];
    return <Section />;
  }
  return (
    <FormScreen>
      <Stack.Screen options={{ title: galleryCopy.title }} />
      <FormSection
        header={galleryCopy.sectionsHeader}
        footer={galleryCopy.sectionsFooter}>
        {(Object.keys(sections) as SectionName[]).map((name) => (
          <NavigationRow
            key={name}
            title={galleryCopy.sections[name]}
            testID={`gallery.${name}`}
            onPress={() => {
              router.push({
                pathname: "/dev/gallery",
                params: { section: name },
              });
            }}
          />
        ))}
      </FormSection>
    </FormScreen>
  );
}
