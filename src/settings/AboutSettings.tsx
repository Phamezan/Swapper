import { SwapperDoctor } from "./SwapperDoctor";
import { UpdateSection } from "./UpdateSection";

export function AboutSettings({ remoteTransport }: { remoteTransport: "lan" | "tailscale" }) {
  return (
    <>
      <UpdateSection />
      <SwapperDoctor remoteTransport={remoteTransport} />
    </>
  );
}
