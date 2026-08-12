import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { cn } from "@tessera/ui/lib/utils";
import { motion } from "motion/react";
import type { Variants } from "motion/react";

const containerVariants: Variants = {
  hidden: {},
  visible: {
    transition: { delayChildren: 0.1, staggerChildren: 0.06 },
  },
};

const itemVariants: Variants = {
  hidden: { opacity: 0, y: 10 },
  visible: {
    opacity: 1,
    transition: { duration: 0.5, ease: [0.16, 1, 0.3, 1] },
    y: 0,
  },
};

export function LoginForm({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div className={cn("flex flex-col gap-6", className)} {...props}>
      <motion.div
        animate="visible"
        initial="hidden"
        variants={containerVariants}
      >
        <motion.div variants={itemVariants}>
          <Card className="border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
            <CardHeader>
              <motion.div
                className="mb-2 flex items-center gap-2"
                variants={itemVariants}
              >
                <span className="relative flex h-1.5 w-1.5">
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary/60" />
                  <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
                </span>
                <span className="text-muted-foreground text-xs">
                  Receiving space is open
                </span>
              </motion.div>
              <motion.div variants={itemVariants}>
                <CardTitle className="font-medium text-xl tracking-tight">
                  Welcome back
                </CardTitle>
              </motion.div>
              <motion.div variants={itemVariants}>
                <CardDescription>
                  Sign in to send and receive files, snippets, and links.
                </CardDescription>
              </motion.div>
            </CardHeader>
            <CardContent>
              <form>
                <FieldGroup>
                  <motion.div variants={itemVariants}>
                    <Field>
                      <FieldLabel htmlFor="email">Email</FieldLabel>
                      <Input
                        id="email"
                        placeholder="m@example.com"
                        required
                        type="email"
                      />
                    </Field>
                  </motion.div>
                  <motion.div variants={itemVariants}>
                    <Field>
                      <div className="flex items-center">
                        <FieldLabel htmlFor="password">Password</FieldLabel>
                        <a
                          className="ml-auto inline-block text-sm underline-offset-4 hover:underline"
                          href="#"
                        >
                          Forgot your password?
                        </a>
                      </div>
                      <Input id="password" required type="password" />
                    </Field>
                  </motion.div>
                  <motion.div variants={itemVariants}>
                    <Field>
                      <motion.div whileTap={{ scale: 0.98 }}>
                        <Button className="w-full" type="submit">
                          Sign in
                        </Button>
                      </motion.div>
                      <motion.div whileTap={{ scale: 0.98 }}>
                        <Button
                          className="w-full"
                          type="button"
                          variant="outline"
                        >
                          Login with Google
                        </Button>
                      </motion.div>
                      <FieldDescription className="text-center">
                        Don&apos;t have an account? <a href="#">Sign up</a>
                      </FieldDescription>
                    </Field>
                  </motion.div>
                </FieldGroup>
              </form>
            </CardContent>
          </Card>
        </motion.div>
      </motion.div>
    </div>
  );
}
