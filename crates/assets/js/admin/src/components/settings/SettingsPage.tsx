import { persistentAtom } from "@nanostores/persistent";
import { createSignal, For, createEffect } from "solid-js";
import type { Component, JSX } from "solid-js";
import { useParams, useNavigate } from "@solidjs/router";
import {
  TbOutlineBriefcase,
  TbOutlineCloudUpload,
  TbOutlineDatabaseExport,
  TbOutlineDeviceFloppy,
  TbOutlineMail,
  TbOutlineRefresh,
  TbOutlineServer,
  TbOutlineTable,
  TbOutlineUser,
} from "solid-icons/tb";
import { IconProps } from "solid-icons";
import { useQueryClient } from "@tanstack/solid-query";

import { Dialog } from "@/components/ui/dialog";
import { showToast } from "@/components/ui/toast";
import {
  useSidebar,
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
} from "@/components/ui/sidebar";

import { Header } from "@/components/Header";
import { ConfirmCloseDialog } from "@/components/SafeSheet";
import { AuthSettings } from "@/components/settings/AuthSettings";
import { DatabaseSettings } from "@/components/settings/DatabaseSettings";
import { SchemaSettings } from "@/components/settings/SchemaSettings";
import { EmailSettings } from "@/components/settings/EmailSettings";
import { FileSettings } from "@/components/settings/FileSettings";
import { JobSettings } from "@/components/settings/JobSettings";
import { BackupSettings } from "@/components/settings/BackupSettings";
import { ServerSettings } from "@/components/settings/ServerSettings";
import { IconButton } from "@/components/IconButton";

import { invalidateConfig } from "@/lib/api/config";

export interface CommonProps {
  markDirty: () => void;
  postSubmit: () => void;
}

interface Site {
  route: string;
  label: string;
  child: Component<CommonProps>;
  icon: (props: IconProps) => JSX.Element;
}

const sites: Site[] = [
  {
    route: "host",
    label: "Host",
    child: ServerSettings,
    icon: TbOutlineServer,
  },
  {
    route: "email",
    label: "Email",
    child: EmailSettings,
    icon: TbOutlineMail,
  },
  {
    route: "auth",
    label: "Auth",
    child: AuthSettings,
    icon: TbOutlineUser,
  },
  {
    route: "backup",
    label: "Backups",
    child: BackupSettings,
    icon: TbOutlineDeviceFloppy,
  },
  {
    route: "jobs",
    label: "Jobs",
    child: JobSettings,
    icon: TbOutlineBriefcase,
  },
  {
    route: "data",
    label: "Databases",
    child: DatabaseSettings,
    icon: TbOutlineDatabaseExport,
  },
  {
    route: "files",
    label: "Files",
    child: FileSettings,
    icon: TbOutlineCloudUpload,
  },
  {
    route: "schema",
    label: "Schemas",
    child: SchemaSettings,
    icon: TbOutlineTable,
  },
] as const;

export function SettingsPage() {
  const queryClient = useQueryClient();
  const params = useParams<{ group: string }>();
  const navigate = useNavigate();

  const [dirty, setDirty] = createSignal(false);
  const [dirtyDialog, setDirtyDialog] = createSignal<
    DirtyDialogState | undefined
  >();

  const activeSite = () => {
    const g = params?.group;
    if (g) {
      return sites.find((s) => s.route == g) ?? sites[0];
    }

    const index = $uiState.get().selected;
    return sites[index === undefined || index >= sites.length ? 0 : index];
  };

  createEffect(() => {
    const g = params?.group;
    if (!g) {
      return;
    }

    const index = sites.findIndex((s) => s.route == g);
    if (index >= 0) {
      $uiState.set({
        ...$uiState.get(),
        selected: index,
      });
    }
  });

  const p = () =>
    ({
      markDirty: () => setDirty(true),
      postSubmit: () => {
        setDirty(false);
        showToast({
          title: "submitted",
          variant: "success",
        });
      },
    }) as CommonProps;

  const Body = () => (
    <Dialog
      id="switch-settings-dialog"
      open={dirtyDialog() !== undefined}
      onOpenChange={(isOpen) => {
        if (!isOpen) {
          setDirtyDialog();
        }
      }}
      modal={true}
    >
      <ConfirmCloseDialog
        back={() => setDirtyDialog()}
        confirm={() => {
          const state = dirtyDialog();
          if (state) {
            setDirtyDialog();
            setDirty(false);
            navigate("/settings/" + state.nextRoute);
          }
        }}
      />

      <Header
        title="Settings"
        titleSelect={activeSite().label}
        left={
          <IconButton onClick={() => invalidateConfig(queryClient)}>
            <TbOutlineRefresh />
          </IconButton>
        }
      />

      <div class="m-4">{activeSite().child(p())}</div>
    </Dialog>
  );

  return (
    <div class="size-full scrollbar-thin md:overflow-y-auto">
      <SidebarProvider>
        <Sidebar
          class="absolute"
          variant="sidebar"
          side="left"
          collapsible="offcanvas"
        >
          <SidebarContent>
            <SidebarGroup>
              <SettingsSidebar
                activeRoute={activeSite().route}
                dirty={dirty()}
                openDirtyDialog={setDirtyDialog}
              />
            </SidebarGroup>

            {/* <SidebarFooter /> */}
          </SidebarContent>

          <SidebarRail />
        </Sidebar>

        <SidebarInset>
          <Body />
        </SidebarInset>
      </SidebarProvider>
    </div>
  );
}

type DirtyDialogState = {
  nextRoute: string;
};

function SettingsSidebar(props: {
  activeRoute: string | undefined;
  dirty: boolean;
  openDirtyDialog: (s: DirtyDialogState) => void;
}) {
  const { setOpenMobile } = useSidebar();
  const navigate = useNavigate();

  return (
    <div class="p-2">
      <SidebarGroupContent>
        <SidebarMenu>
          <For each={sites}>
            {(s: Site) => {
              const match = () => props.activeRoute === s.route;

              return (
                <SidebarMenuItem>
                  <SidebarMenuButton
                    isActive={match()}
                    size="md"
                    variant="default"
                    onClick={() => {
                      setOpenMobile(false);
                      if (match()) {
                        // Nothing to do.
                        return;
                      }

                      if (!props.dirty) {
                        navigate("/settings/" + s.route);
                        return;
                      }

                      // Open a dirty warning.
                      props.openDirtyDialog({
                        nextRoute: s.route,
                      });
                    }}
                  >
                    {<s.icon />}

                    {s.label}
                  </SidebarMenuButton>
                </SidebarMenuItem>
              );
            }}
          </For>
        </SidebarMenu>
      </SidebarGroupContent>
    </div>
  );
}

type SettingsUiState = {
  selected?: number;
};

const $uiState = persistentAtom<SettingsUiState>(
  "settings:state",
  {},
  {
    encode: JSON.stringify,
    decode: JSON.parse,
  },
);
