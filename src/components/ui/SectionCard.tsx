import { Box, Group, Paper, Text } from '@mantine/core';

interface SectionCardProps {
  icon: React.ComponentType<{ size?: number }>;
  title: string;
  headerRight?: React.ReactNode;
  children: React.ReactNode;
}

export function SectionCard({ icon: Icon, title, headerRight, children }: SectionCardProps) {
  return (
    <Paper p="lg" radius="md" withBorder shadow="xs">
      <Group gap={8} mb="md" justify="space-between">
        <Group gap={8}>
          <Box style={{ color: 'var(--mantine-primary-color-filled)', opacity: 0.7, display: 'flex' }}>
            <Icon size={15} />
          </Box>
          <Text size="xs" fw={600} tt="uppercase" c="dimmed" style={{ letterSpacing: '0.06em' }}>
            {title}
          </Text>
        </Group>
        {headerRight}
      </Group>
      {children}
    </Paper>
  );
}
