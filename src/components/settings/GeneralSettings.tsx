import { Box, Stack } from '@mantine/core';
import { Keyboard, Type, Rocket } from 'lucide-react';
import { SectionCard } from '../ui/SectionCard';
import { HotkeySettings } from './HotkeySettings';
import { OutputSettings } from './OutputSettings';
import { StartupSettings } from './StartupSettings';

export function GeneralSettings() {
  return (
    <Box p="xl" maw={640} mx="auto">
      <Stack gap="lg">
        <SectionCard icon={Keyboard} title="Hotkey & Dictation">
          <HotkeySettings />
        </SectionCard>

        <SectionCard icon={Type} title="Output">
          <OutputSettings />
        </SectionCard>

        <SectionCard icon={Rocket} title="Preferences">
          <StartupSettings />
        </SectionCard>
      </Stack>
    </Box>
  );
}
