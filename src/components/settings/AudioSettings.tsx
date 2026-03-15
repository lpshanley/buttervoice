import { Box, Stack } from '@mantine/core';
import { Activity } from 'lucide-react';
import { SectionCard } from '../ui/SectionCard';
import { SignalProcessing } from './SignalProcessing';

export function AudioSettings() {
  return (
    <Box p="xl" maw={640} mx="auto">
      <Stack gap="lg">
        <SectionCard icon={Activity} title="Signal Processing">
          <SignalProcessing />
        </SectionCard>
      </Stack>
    </Box>
  );
}
