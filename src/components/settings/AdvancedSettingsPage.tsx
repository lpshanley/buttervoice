import { Box, Stack } from '@mantine/core';
import { Bug, FlaskConical } from 'lucide-react';
import { SectionCard } from '../ui/SectionCard';
import { AdvancedSettings } from './AdvancedSettings';
import { BetaSettings } from './BetaSettings';

export function AdvancedSettingsPage() {
  return (
    <Box p="xl" maw={640} mx="auto">
      <Stack gap="lg">
        <SectionCard icon={Bug} title="Debugging">
          <AdvancedSettings />
        </SectionCard>
        <SectionCard icon={FlaskConical} title="Beta">
          <BetaSettings />
        </SectionCard>
      </Stack>
    </Box>
  );
}
