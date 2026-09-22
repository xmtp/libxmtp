import { useDisclosure } from "@mantine/hooks";
import { useEffect } from "react";
import { Outlet, useLocation, useNavigate } from "react-router";

import { AppFooter } from "@/components/App/AppFooter";
import { AppHeader } from "@/components/App/AppHeader";
import { Disclaimer } from "@/components/App/Disclaimer";
import { ConversationsNavbar } from "@/components/Conversations/ConversationsNavbar";
import { useXMTP } from "@/contexts/XMTPContext";
import { useRedirect } from "@/hooks/useRedirect";
import {
  MainLayout,
  MainLayoutContent,
  MainLayoutFooter,
  MainLayoutHeader,
  MainLayoutNav,
} from "@/layouts/MainLayout";

export const AppLayout: React.FC = () => {
  const navigate = useNavigate();
  const location = useLocation();
  const { client } = useXMTP();
  const { setRedirectUrl } = useRedirect();
  const [opened, { toggle }] = useDisclosure();

  useEffect(() => {
    if (!client) {
      // save the current path to redirect to it after the client is initialized
      if (location.pathname !== "/" && location.pathname !== "/disconnect") {
        setRedirectUrl(`${location.pathname}${location.search}`);
      }
      void navigate("/");
    }
  }, [client, location.pathname, location.search, navigate, setRedirectUrl]);

  return client ? (
    <>
      <MainLayout>
        <MainLayoutHeader>
          <AppHeader client={client} opened={opened} toggle={toggle} />
        </MainLayoutHeader>
        <MainLayoutNav opened={opened} toggle={toggle}>
          <ConversationsNavbar />
        </MainLayoutNav>
        <MainLayoutContent>
          <Outlet />
        </MainLayoutContent>
        <MainLayoutFooter>
          <AppFooter />
        </MainLayoutFooter>
      </MainLayout>
      <Disclaimer />
    </>
  ) : null;
};
