import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  sidecarStatus,
  sidecarStart,
  sidecarRestart,
  sidecarStop,
  type SidecarSnapshot,
} from "../api/commands";

const STATUS_KEY = ["sidecar", "status"] as const;

/** Poll sidecar status every 3 seconds while the window is focused. */
export function useSidecarStatus() {
  return useQuery<SidecarSnapshot>({
    queryKey: STATUS_KEY,
    queryFn: sidecarStatus,
    refetchInterval: 3_000,
  });
}

export function useSidecarStart() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: sidecarStart,
    onSuccess: (snapshot) => {
      qc.setQueryData(STATUS_KEY, snapshot);
    },
  });
}

export function useSidecarRestart() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: sidecarRestart,
    onSuccess: (snapshot) => {
      qc.setQueryData(STATUS_KEY, snapshot);
    },
  });
}

export function useSidecarStop() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: sidecarStop,
    onSuccess: (snapshot) => {
      qc.setQueryData(STATUS_KEY, snapshot);
    },
  });
}
