import { NextResponse } from "next/server";
import { db } from "@/lib/db";

type Context = { params: Promise<{ id: string }> };

export async function GET(_request: Request, { params }: Context) {
  const { id } = await params;
  const book = await db.book.findUnique({ where: { id } });

  if (!book) {
    return NextResponse.json({ error: "Book not found" }, { status: 404 });
  }

  return NextResponse.json(book);
}

export async function PATCH(request: Request, { params }: Context) {
  const { id } = await params;
  const { title, author } = await request.json();

  const book = await db.book.update({
    where: { id },
    data: { title, author },
  });

  return NextResponse.json(book);
}
