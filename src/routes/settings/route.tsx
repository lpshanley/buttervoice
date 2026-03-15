import { createFileRoute, Outlet, redirect } from '@tanstack/react-router';
import { Box } from '@mantine/core';

export const Route = createFileRoute('/settings')({
  beforeLoad: ({ location }) => {
    if (location.pathname === '/settings') {
      throw redirect({ to: '/settings/general' });
    }
  },
  component: SettingsLayout,
});

function SettingsLayout() {
  return (
    <Box p="lg">
      <Outlet />
    </Box>
  );
}
